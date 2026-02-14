use super::session::Session;
use crate::domain::permissions::store::SqlitePermissionStore;
use crate::domain::permissions::{PermissionChecker, PermissionConfig, PermissionPrompter};
use crate::domain::todo::TodoList;
use crate::domain::plan::PlanService;
use crate::domain::workflow::{CancellationToken, Chain, Error as WorkflowError, Workflow};
use crate::domain::UserSettings;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{AgentToUiEvent, PermissionUpdate};
use crate::infrastructure::InferenceEngine;
use crate::repository::{
    ModelsRepository, SessionRequestStepsRepository, SessionRequestsRepository, TodoListRepository,
    UserSettingsRepository,
};
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;

/// Service for running workflows on sessions
pub struct SessionService {
    workflow: Workflow,
    use_behavior_trees: bool,
    conn: DbPool,
    event_sender: Sender<AgentToUiEvent>,
    confirmation_rx: Option<Receiver<PermissionUpdate>>,
}

impl SessionService {
    pub fn new(
        engine: Arc<dyn InferenceEngine>,
        conn: DbPool,
        use_behavior_trees: bool,
        permission_config: PermissionConfig,
        prompter: Arc<dyn PermissionPrompter>,
        event_sender: Sender<AgentToUiEvent>,
        confirmation_rx: Option<Receiver<PermissionUpdate>>,
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
            event_sender.clone(),
            conn.clone(),
        )
        .map_err(|err| format!("Failed to create workflow: {}", err))?;

        Ok(Self {
            workflow,
            use_behavior_trees,
            conn,
            event_sender,
            confirmation_rx,
        })
    }

    /// Main entry point — thin router that dispatches to build() or build_from_plan()
    pub fn run(
        &mut self,
        session: &Session,
        prompt: &str,
        images: &[String],
        mode: crate::domain::AgentModeType,
        cancel: &CancellationToken,
    ) -> Result<Chain, ServiceError> {
        // Get user settings and model name
        let (settings_row, model_name) = {
            let conn = self.conn.get().map_err(|e| {
                ServiceError::Repository(format!("Failed to get database connection: {}", e))
            })?;

            let settings_repo = UserSettingsRepository::new(&*conn);
            let settings_row = settings_repo
                .get_current()
                .map_err(|e| ServiceError::Repository(e))?;
            let model_name = settings_row
                .current_model_id
                .and_then(|id| ModelsRepository::new(&*conn).find_by_id(id).ok())
                .flatten()
                .and_then(|model| model.model_name);
            (settings_row, model_name)
        };

        let user_settings =
            UserSettings::from(settings_row.clone()).with_current_model_name(model_name);

        // Safeguard: if BuildFromPlan is requested but no valid TODO list exists,
        // fall back to Build mode to avoid a broken state.
        let effective_mode = if mode == crate::domain::AgentModeType::BuildFromPlan {
            let has_pending = {
                let conn = self.conn.get().map_err(|e| {
                    ServiceError::Repository(format!("Failed to get database connection: {}", e))
                })?;
                let repo = TodoListRepository::new(&*conn);
                match repo.get_by_session(session.id()) {
                    Ok(Some(row)) => match serde_json::from_str::<TodoList>(&row.content) {
                        Ok(todo_list) => !todo_list.is_completed() && !todo_list.items.is_empty(),
                        Err(_) => false,
                    },
                    _ => false,
                }
            };
            if has_pending {
                crate::domain::AgentModeType::BuildFromPlan
            } else {
                crate::domain::AgentModeType::Build
            }
        } else {
            mode
        };

        // Route based on effective mode
        if effective_mode == crate::domain::AgentModeType::BuildFromPlan {
            self.build_from_plan(session, images, &user_settings, &settings_row, cancel)
        } else {
            self.build(
                session,
                prompt,
                images,
                &user_settings,
                effective_mode,
                cancel,
            )
        }
    }

    /// Run a single workflow for one prompt. Creates its own SessionRequest in DB.
    pub(crate) fn build(
        &mut self,
        session: &Session,
        prompt: &str,
        images: &[String],
        user_settings: &UserSettings,
        mode: crate::domain::AgentModeType,
        cancel: &CancellationToken,
    ) -> Result<Chain, ServiceError> {
        // Create a new session request
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let request_row = requests_repo
            .create(session.id(), prompt, mode)
            .map_err(|e| ServiceError::Repository(e))?;
        let request_id = request_row.id;

        // Set up request context
        let mut request = session.clone();
        request.set_current_request(prompt.to_string());
        request.set_current_images(images.to_vec());
        request.set_current_user_settings(Some(user_settings.clone()));
        request.set_current_mode(mode);
        request.set_conn(self.conn.clone());

        // Run the workflow
        let result: Result<Chain, WorkflowError> = if self.use_behavior_trees {
            self.workflow.run_using_bt(&mut request, cancel)
        } else {
            self.workflow
                .run(&mut request, cancel, mode)
                .map(|_| self.workflow.get_chain().clone())
        };

        match result {
            Ok(chain) => {
                // Get summary from chain
                let summary = chain.get_summary();

                // Update the request with result_summary
                requests_repo
                    .update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;

                // Aggregate file changes from patch_files steps
                let merged_changes = chain.merged_file_changes();

                if !merged_changes.is_empty() {
                    let changes_json = serde_json::json!({
                        "changes": merged_changes.clone()
                    })
                    .to_string();
                    requests_repo
                        .update_file_changes(request_id, &changes_json)
                        .map_err(|e| ServiceError::Repository(e))?;

                    // Emit FileChangesEvent
                    let _ = self.event_sender.send(AgentToUiEvent::FileChangesEvent {
                        request_id,
                        changes: merged_changes,
                    });
                }

                // Save chain steps to database for future requests
                let steps_repo = SessionRequestStepsRepository::new(self.conn.clone());
                steps_repo
                    .save_steps_for_request(request_id, chain.get_steps())
                    .map_err(|e| {
                        log::error!("Failed to save steps for request {}: {}", request_id, e);
                        ServiceError::Repository(format!("Failed to save steps: {}", e))
                    })?;

                Ok(chain)
            }
            Err(e) => {
                // Use the actual workflow chain (which has steps executed before failure)
                let mut chain = self.workflow.get_chain().clone();
                chain.mark_failed(format!("Error: {}", e));

                let summary = chain.get_summary();

                requests_repo
                    .update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;

                // Save chain steps to database even on failure
                let steps_repo = SessionRequestStepsRepository::new(self.conn.clone());
                if let Err(save_err) =
                    steps_repo.save_steps_for_request(request_id, chain.get_steps())
                {
                    log::error!(
                        "Failed to save steps for failed request {}: {}",
                        request_id,
                        save_err
                    );
                }

                Err(ServiceError::Workflow(e))
            }
        }
    }

    /// Sub-agent orchestrator: runs each pending TODO item as its own build() call
    fn build_from_plan(
        &mut self,
        session: &Session,
        images: &[String],
        user_settings: &UserSettings,
        _settings_row: &crate::repository::UserSettingsRow,
        cancel: &CancellationToken,
    ) -> Result<Chain, ServiceError> {
        let todo_list = match self.load_todo_list(session.id()) {
            Some(list) if !list.items.is_empty() && !list.is_completed() => list,
            _ => {
                let mut chain = Chain::new();
                chain.set_final_message("No pending TODO items to execute.".to_string());
                return Ok(chain);
            }
        };

        let plan = match PlanService::from_todo_list(session.id(), todo_list) {
            Some(plan) => plan,
            None => {
                let mut chain = Chain::new();
                chain.set_final_message("No pending TODO items to execute.".to_string());
                return Ok(chain);
            }
        };

        let plan_service = PlanService::new(
            self.conn.clone(),
            self.event_sender.clone(),
            self.confirmation_rx.clone(),
        );

        plan_service.execute(
            self,
            plan,
            session,
            images,
            user_settings,
            cancel,
        )
    }

    /// Load the TODO-list for a session from the database
    fn load_todo_list(&self, session_id: i64) -> Option<TodoList> {
        let conn = self.conn.get().ok()?;
        let repo = TodoListRepository::new(&*conn);
        let row = repo.get_by_session(session_id).ok()??;
        serde_json::from_str::<TodoList>(&row.content).ok()
    }

}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("workflow error: {0}")]
    Workflow(WorkflowError),
    #[error("repository error: {0}")]
    Repository(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::permissions::{PermissionRequest, UserPermissionDecision};
    use crate::domain::todo::{TodoItem, TodoList, TodoListStatus};
    use crate::domain::workflow::chain::Chain;
    use crate::domain::{AgentModeType, ModelType, Project};
    use crate::infrastructure::db;
    use crate::infrastructure::event_bus::{AgentToUiEvent, PermissionUpdate};
    use crate::infrastructure::inference::{InferenceEngine, LLMInferenceResult};
    use crate::infrastructure::InfaError;
use crate::repository::{ProjectsRepository, SessionsRepository, TodoListRepository, UserSettingsRepository};
    use crossbeam_channel::{unbounded, Receiver, Sender};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_app_name() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("quillcode-test-session-{}", nanos)
    }

    /// Mock inference engine that returns a simple text response (no tool_call)
    struct MockInferenceEngine;

    impl InferenceEngine for MockInferenceEngine {
        fn generate(
            &self,
            _tools: &[&dyn crate::domain::tools::Tool],
            _chain: &Chain,
            _images: &[String],
            _tracer: Option<&mut openai_agents_tracing::TracingFacade>,
        ) -> Result<LLMInferenceResult, InfaError> {
            Ok(LLMInferenceResult {
                summary: "Task completed successfully.".to_string(),
                raw_output: "Done.".to_string(),
                tool_call: None,
            })
        }

        fn get_type(&self) -> ModelType {
            ModelType::OpenAI
        }
    }

    /// Mock permission prompter that always returns AllowOnce
    struct MockPermissionPrompter;

    impl PermissionPrompter for MockPermissionPrompter {
        fn ask_permission(
            &self,
            _request: &PermissionRequest,
        ) -> Result<UserPermissionDecision, crate::utils::AskError> {
            Ok(UserPermissionDecision::AllowOnce)
        }
    }

    /// Create a test session in the database and return the Session entity
    fn create_test_session(conn: &crate::infrastructure::db::DbPool) -> Session {
        let conn_guard = conn.get().expect("get connection");

        // Ensure settings exist
        let settings_repo = UserSettingsRepository::new(&*conn_guard);
        let _ = settings_repo.get_current().expect("init settings");

        let projects_repo = ProjectsRepository::new(&*conn_guard);
        let project_row = projects_repo
            .create("test-project", "/tmp/test-project")
            .expect("create project");

        let sessions_repo = SessionsRepository::new(&*conn_guard);
        let session_row = sessions_repo
            .create(project_row.id, "test-session")
            .expect("create session");

        let project = Project::from(project_row);
        Session::from_row_with_project(session_row, project)
    }

    /// Populate a TODO list in the database for the given session
    fn create_todo_list(
        conn: &crate::infrastructure::db::DbPool,
        session_id: i64,
        items: Vec<TodoItem>,
    ) {
        let conn_guard = conn.get().expect("get connection");
        let repo = TodoListRepository::new(&*conn_guard);
        let row = repo
            .get_or_create_for_session(session_id)
            .expect("create todo list");
        let todo_list = TodoList { items };
        let json = serde_json::to_string(&todo_list).expect("serialize todo list");
        repo.update_content(row.id, &json)
            .expect("update todo list");
    }

    fn create_session_service(
        conn: &crate::infrastructure::db::DbPool,
        event_sender: Sender<AgentToUiEvent>,
        confirmation_rx: Option<Receiver<PermissionUpdate>>,
    ) -> SessionService {
        let engine: Arc<dyn InferenceEngine> = Arc::new(MockInferenceEngine);
        let prompter: Arc<dyn PermissionPrompter> = Arc::new(MockPermissionPrompter);

        SessionService::new(
            engine,
            conn.clone(),
            false, // use_behavior_trees
            PermissionConfig::default(),
            prompter,
            event_sender,
            confirmation_rx,
        )
        .expect("create session service")
    }

    fn pending_item(title: &str, desc: &str) -> TodoItem {
        TodoItem {
            title: title.to_string(),
            description: desc.to_string(),
            status: TodoListStatus::Pending,
        }
    }

    fn completed_item(title: &str, desc: &str) -> TodoItem {
        TodoItem {
            title: title.to_string(),
            description: desc.to_string(),
            status: TodoListStatus::Completed,
        }
    }

    /// Load the current TODO list from DB for assertions
    fn load_todo_list(conn: &crate::infrastructure::db::DbPool, session_id: i64) -> TodoList {
        let conn_guard = conn.get().expect("get connection");
        let repo = TodoListRepository::new(&*conn_guard);
        let row = repo
            .get_by_session(session_id)
            .expect("get todo list")
            .expect("todo list exists");
        serde_json::from_str(&row.content).expect("parse todo list")
    }

    // ─── Test Cases ───

    #[test]
    fn run_routes_build_mode() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();

        let mut service = create_session_service(&conn, event_tx, None);
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        let result = service.run(
            &session,
            "Hello, build something",
            &[],
            AgentModeType::Build,
            &cancel,
        );

        let chain = result.expect("build should succeed");
        assert!(!chain.is_failed);
    }

    #[test]
    fn run_build_from_plan_falls_back_when_no_todo() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();

        let mut service = create_session_service(&conn, event_tx, None);
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        // No TODO list in DB → should fall back to Build mode
        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        let chain = result.expect("fallback build should succeed");
        assert!(!chain.is_failed);
    }

    #[test]
    fn run_build_from_plan_falls_back_when_all_completed() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();

        let mut service = create_session_service(&conn, event_tx, None);
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        // All items completed
        create_todo_list(
            &conn,
            session.id(),
            vec![
                completed_item("Task 1", "Done already"),
                completed_item("Task 2", "Also done"),
            ],
        );

        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        let chain = result.expect("fallback build should succeed");
        assert!(!chain.is_failed);
    }

    #[test]
    fn build_from_plan_executes_pending_items() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();
        let (confirm_tx, confirm_rx) = unbounded::<PermissionUpdate>();

        let mut service = create_session_service(&conn, event_tx, Some(confirm_rx));
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![
                pending_item("Task 1", "First task"),
                pending_item("Task 2", "Second task"),
            ],
        );

        // Auto-approve confirmations between items
        std::thread::spawn(move || {
            // After item 1 completes, a confirmation with request_id 1_000_001 is sent
            let _ = confirm_tx.send(PermissionUpdate {
                request_id: 1_000_001,
                decision: UserPermissionDecision::AllowOnce,
            });
        });

        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        let chain = result.expect("build_from_plan should succeed");
        assert!(!chain.is_failed);

        // Both items should be completed
        let todo = load_todo_list(&conn, session.id());
        assert_eq!(todo.items.len(), 2);
        assert_eq!(todo.items[0].status, TodoListStatus::Completed);
        assert_eq!(todo.items[1].status, TodoListStatus::Completed);
    }

    #[test]
    fn build_from_plan_stops_on_deny() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();
        let (confirm_tx, confirm_rx) = unbounded::<PermissionUpdate>();

        let mut service = create_session_service(&conn, event_tx, Some(confirm_rx));
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![
                pending_item("Task 1", "First task"),
                pending_item("Task 2", "Second task"),
                pending_item("Task 3", "Third task"),
            ],
        );

        // Deny the first confirmation
        std::thread::spawn(move || {
            let _ = confirm_tx.send(PermissionUpdate {
                request_id: 1_000_001,
                decision: UserPermissionDecision::Deny,
            });
        });

        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        let chain = result.expect("deny should return Ok with stop message");
        assert!(
            chain
                .final_message
                .as_ref()
                .map(|m| m.contains("Stopped by user"))
                .unwrap_or(false),
            "Expected 'Stopped by user' in final_message, got {:?}",
            chain.final_message
        );

        // Only first item should be completed
        let todo = load_todo_list(&conn, session.id());
        assert_eq!(todo.items[0].status, TodoListStatus::Completed);
        assert_eq!(todo.items[1].status, TodoListStatus::Pending);
        assert_eq!(todo.items[2].status, TodoListStatus::Pending);
    }

    #[test]
    fn build_from_plan_skips_completed_items() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();
        let (confirm_tx, confirm_rx) = unbounded::<PermissionUpdate>();

        let mut service = create_session_service(&conn, event_tx, Some(confirm_rx));
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![
                completed_item("Task 1", "Already done"),
                pending_item("Task 2", "Second task"),
                pending_item("Task 3", "Third task"),
            ],
        );

        // Auto-approve between the 2 pending items
        std::thread::spawn(move || {
            let _ = confirm_tx.send(PermissionUpdate {
                request_id: 1_000_001,
                decision: UserPermissionDecision::AllowOnce,
            });
        });

        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        let chain = result.expect("should succeed");
        assert!(!chain.is_failed);

        // All items should now be completed (first was already, other two were executed)
        let todo = load_todo_list(&conn, session.id());
        assert_eq!(todo.items[0].status, TodoListStatus::Completed);
        assert_eq!(todo.items[1].status, TodoListStatus::Completed);
        assert_eq!(todo.items[2].status, TodoListStatus::Completed);
    }

    #[test]
    fn build_from_plan_sends_confirmation_events() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, event_rx) = unbounded();
        let (confirm_tx, confirm_rx) = unbounded::<PermissionUpdate>();

        let mut service = create_session_service(&conn, event_tx, Some(confirm_rx));
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![
                pending_item("Task 1", "First task"),
                pending_item("Task 2", "Second task"),
            ],
        );

        // Auto-approve
        std::thread::spawn(move || {
            let _ = confirm_tx.send(PermissionUpdate {
                request_id: 1_000_001,
                decision: UserPermissionDecision::AllowOnce,
            });
        });

        let _ = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        // Collect all events
        drop(service);
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Find PermissionRequestEvent with tool_name "build_from_plan"
        let confirmation_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, AgentToUiEvent::PermissionRequestEvent { tool_name, .. } if tool_name == "build_from_plan"))
            .collect();

        assert!(
            !confirmation_events.is_empty(),
            "Expected at least one PermissionRequestEvent with tool_name 'build_from_plan'"
        );
    }

    #[test]
    fn build_from_plan_respects_cancellation() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, _event_rx) = unbounded();

        let mut service = create_session_service(&conn, event_tx, None);
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![pending_item("Task 1", "First task")],
        );

        // Cancel before running
        cancel.cancel();

        let result = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        assert!(
            matches!(
                result,
                Err(ServiceError::Workflow(WorkflowError::Cancelled))
            ),
            "Expected Cancelled error, got {:?}",
            result
        );
    }

    #[test]
    fn build_from_plan_emits_todo_update_events() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let (event_tx, event_rx) = unbounded();
        let (confirm_tx, confirm_rx) = unbounded::<PermissionUpdate>();

        let mut service = create_session_service(&conn, event_tx, Some(confirm_rx));
        let session = create_test_session(&conn);
        let cancel = CancellationToken::new();

        create_todo_list(
            &conn,
            session.id(),
            vec![
                pending_item("Task 1", "First task"),
                pending_item("Task 2", "Second task"),
            ],
        );

        // Auto-approve
        std::thread::spawn(move || {
            let _ = confirm_tx.send(PermissionUpdate {
                request_id: 1_000_001,
                decision: UserPermissionDecision::AllowOnce,
            });
        });

        let _ = service.run(
            &session,
            "Build from plan",
            &[],
            AgentModeType::BuildFromPlan,
            &cancel,
        );

        // Collect all events
        drop(service);
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Find TodoListUpdateEvent emissions
        let todo_update_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, AgentToUiEvent::TodoListUpdateEvent { .. }))
            .collect();

        assert!(
            todo_update_events.len() >= 2,
            "Expected at least 2 TodoListUpdateEvent, got {}",
            todo_update_events.len()
        );
    }
}