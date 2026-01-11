use crate::domain::workflow::{Workflow, Chain, CancellationToken, Error as WorkflowError};
use super::session::Session;
use crate::infrastructure::inference::InferenceEngine;
use crate::repository::SessionRequestsRepository;
use rusqlite::Connection;

/// Service for running workflows on sessions
pub struct SessionService {
    workflow: Workflow,
}

impl SessionService {
    /// Create a new session service with default workflow
    pub fn new() -> Self {
        Self {
            workflow: Workflow::new(),
        }
    }

    /// Create a session service with a custom workflow
    pub fn with_workflow(workflow: Workflow) -> Self {
        Self { workflow }
    }

    /// Run the workflow for a new request in the given session
    /// This is the main entry point for executing a coding task
    /// Creates a new session_request, runs the workflow, and updates the request with the result
    /// Returns the execution chain
    pub fn run(
        &self,
        session: &Session,
        prompt: &str,
        engine: &InferenceEngine,
        cancel: &CancellationToken,
        conn: &Connection,
    ) -> Result<Chain, ServiceError> {
        // Create a new session request
        let requests_repo = SessionRequestsRepository::new(conn);
        let request_row = requests_repo.create(session.id(), prompt)
            .map_err(|e| ServiceError::Repository(e))?;
        let request_id = request_row.id;

        // Update session with current request for workflow execution
        // We need a mutable session, but we only have a reference
        // The workflow uses session.current_prompt() which needs the current_request set
        // For now, we'll create a temporary session with the current request set
        let mut session_with_request = session.clone();
        session_with_request.set_current_request(prompt.to_string());

        // Run the workflow
        let result = match self.workflow.run(&session_with_request, engine, cancel) {
            Ok(chain) => {
                // Get summary and log from chain
                let summary = chain.get_summary();
                let steps_log = chain.get_log();

                // Update the request with result_summary and steps_log
                requests_repo.update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;
                requests_repo.update_steps_log(request_id, &steps_log)
                    .map_err(|e| ServiceError::Repository(e))?;

                Ok(chain)
            }
            Err(e) => {
                // Create a chain with error
                let mut chain = Chain::new();
                chain.mark_failed(format!("Error: {}", e));
                
                let summary = chain.get_summary();
                let steps_log = chain.get_log();
                
                requests_repo.update_result(request_id, &summary)
                    .map_err(|e| ServiceError::Repository(e))?;
                requests_repo.update_steps_log(request_id, &steps_log)
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

impl Default for SessionService {
    fn default() -> Self {
        Self::new()
    }
}
