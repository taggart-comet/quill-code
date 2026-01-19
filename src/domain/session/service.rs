use super::session::Session;
use crate::domain::workflow::{CancellationToken, Chain, Error as WorkflowError, Workflow};
use crate::infrastructure::InferenceEngine;
use crate::repository::SessionRequestsRepository;
use rusqlite::Connection;
use std::sync::Arc;

/// Service for running workflows on sessions
pub struct SessionService {
    workflow: Workflow,
    use_behavior_trees: bool,
    conn: Arc<Connection>,
}

impl SessionService {
    /// Create a new session service with default workflow
    pub fn new(
        engine: Arc<dyn InferenceEngine>,
        conn: Arc<Connection>,
        use_behavior_trees: bool,
    ) -> Result<Self, String> {
        Ok(Self {
            workflow: Workflow::new(engine)?,
            use_behavior_trees,
            conn,
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
        // Create a new session request
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let request_row = requests_repo
            .create(session.id(), prompt)
            .map_err(|e| ServiceError::Repository(e))?;
        let request_id = request_row.id;

        // Update session with current request for workflow execution
        // We need a mutable session, but we only have a reference
        // The workflow uses session.current_prompt() which needs the current_request set
        // For now, we'll create a temporary session with the current request set
        let mut session_with_request = session.clone();
        session_with_request.set_current_request(prompt.to_string());

        // Run the workflow
        let result: Result<Chain, WorkflowError> = if self.use_behavior_trees {
            self.workflow.reset_chain();
            self.workflow
                .run_using_bt(&mut session_with_request, cancel)
        } else {
            self.workflow.reset_chain();
            self.workflow
                .run(&mut session_with_request, cancel, 128, None)
                .map_err(ServiceError::Workflow)?;
            Ok(self.workflow.chain().clone())
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
