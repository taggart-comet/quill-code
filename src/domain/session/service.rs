use super::session::Session;
use crate::domain::permissions::store::SqlitePermissionStore;
use crate::domain::permissions::{PermissionChecker, PermissionConfig, PermissionPrompter};
use crate::domain::workflow::{CancellationToken, Chain, Error as WorkflowError, Workflow};
use crate::domain::UserSettings;
use crate::infrastructure::{EventBus, InferenceEngine};
use crate::repository::{ModelsRepository, SessionRequestsRepository, UserSettingsRepository};
use rusqlite::Connection;
use std::sync::Arc;

/// Service for running workflows on sessions
pub struct SessionService {
    workflow: Workflow,
    use_behavior_trees: bool,
    conn: Arc<Connection>,
    event_bus: Arc<EventBus>,
}

impl SessionService {
    pub fn new_with_permissions_and_prompter(
        engine: Arc<dyn InferenceEngine>,
        conn: Arc<Connection>,
        use_behavior_trees: bool,
        permission_config: PermissionConfig,
        prompter: Arc<dyn PermissionPrompter>,
        event_bus: Arc<EventBus>,
    ) -> Result<Self, String> {
        let permission_store = Arc::new(SqlitePermissionStore::new(conn.clone()));
        let permission_checker = Arc::new(PermissionChecker::new_with_prompter(
            permission_store,
            permission_config,
            prompter,
        ));

        let workflow = Workflow::new(
            engine,
            permission_checker,
            Some(event_bus.agent_to_ui_tx.clone()),
        )
        .map_err(|err| format!("Failed to create workflow: {}", err))?;

        Ok(Self {
            workflow,
            use_behavior_trees,
            conn,
            event_bus,
        })
    }

    /// Run the workflow for a new request in the given session
    /// This is the main entry point for executing a coding task
    /// Creates a new session_request, runs the workflow, and updates the request with the result
    /// Returns the execution chain
    pub fn run(
        &mut self,
        session: &Session,
        prompt: &str,
        cancel: &CancellationToken,
    ) -> Result<Chain, ServiceError> {
        let settings_repo = UserSettingsRepository::new(&self.conn);
        let settings_row = settings_repo
            .get_current()
            .map_err(|e| ServiceError::Repository(e))?;
        let model_name = settings_row
            .current_model_id
            .and_then(|id| ModelsRepository::new(&self.conn).find_by_id(id).ok())
            .flatten()
            .and_then(|model| model.model_name);
        let request_settings =
            UserSettings::from(settings_row.clone()).with_current_model_name(model_name);

        // Create a new session request
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let request_row = requests_repo
            .create(session.id(), settings_row.id, prompt)
            .map_err(|e| ServiceError::Repository(e))?;
        let request_id = request_row.id;

        // Update session with current request for workflow execution
        // We need a mutable session, but we only have a reference
        // The workflow uses session.current_prompt() which needs the current_request set
        // For now, we'll create a temporary session with the current request set
        let mut session_with_request = session.clone();
        session_with_request.set_current_request(prompt.to_string());
        session_with_request.set_current_user_settings(Some(request_settings));

        // Run the workflow
        let result: Result<Chain, WorkflowError> = if self.use_behavior_trees {
            self.workflow
                .run_using_bt(&mut session_with_request, cancel)
        } else {
            self.workflow
                .run(&mut session_with_request, cancel, 128, None)
                .map_err(ServiceError::Workflow)?;
            Ok(self.workflow.get_chain().clone())
        };

        let result = match result {
            Ok(chain) => {
                // Get summary and log from chain
                let summary = chain.get_summary();
                let steps_log = chain.get_log();

                // Update the request with result_summary and steps_log
                requests_repo
                    .update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;
                requests_repo
                    .update_steps_log(request_id, &steps_log)
                    .map_err(|e| ServiceError::Repository(e))?;

                // Aggregate file changes from patch_files steps
                let file_changes: Vec<_> = chain
                    .steps()
                    .iter()
                    .filter(|step| step.tool_name.as_deref() == Some("patch_files"))
                    .filter_map(|step| step.file_changes.as_ref())
                    .flatten()
                    .cloned()
                    .collect();

                if !file_changes.is_empty() {
                    let changes_json = serde_json::json!({
                        "changes": file_changes.clone()
                    })
                    .to_string();
                    requests_repo
                        .update_file_changes(request_id, &changes_json)
                        .map_err(|e| ServiceError::Repository(e))?;

                    // Emit FileChangesEvent
                    let _ = self.event_bus.agent_to_ui_tx.send(
                        crate::infrastructure::AgentToUiEvent::FileChangesEvent {
                            request_id,
                            changes: file_changes,
                        },
                    );
                }

                Ok(chain)
            }
            Err(e) => {
                // Create a chain with error
                let mut chain = Chain::new();
                chain.mark_failed(format!("Error: {}", e));

                let summary = chain.get_summary();
                let steps_log = chain.get_log();

                requests_repo
                    .update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;
                requests_repo
                    .update_steps_log(request_id, &steps_log)
                    .map_err(|e| ServiceError::Repository(e))?;

                Err(ServiceError::Workflow(e))
            }
        };

        result
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("workflow error: {0}")]
    Workflow(WorkflowError),
    #[error("repository error: {0}")]
    Repository(String),
}
