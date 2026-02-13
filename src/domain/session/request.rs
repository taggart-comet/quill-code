use super::session_request::SessionRequest;
use crate::domain::workflow::step::ChainStep;
use crate::domain::AgentModeType;
use std::path::Path;

/// Trait for request context used by workflow execution.
/// This abstraction allows workflow to work with different request implementations
/// without being tightly coupled to the Session entity.
pub trait Request {
    /// Get the history of previous requests
    #[allow(dead_code)]
    fn history(&self) -> &[SessionRequest];

    /// Get the current request prompt
    fn current_request(&self) -> &str;

    /// Get the current agent mode
    #[allow(dead_code)]
    fn mode(&self) -> AgentModeType;

    /// Get the project root path
    fn project_root(&self) -> &Path;

    /// Get the current user settings context
    fn user_settings(&self) -> Option<&crate::domain::UserSettings>;

    /// Get the project ID for permission checks
    fn project_id(&self) -> Option<i32>;

    /// Store a final message produced by the workflow
    fn set_final_message(&mut self, message: String);

    /// Get attached images (data URLs)
    fn images(&self) -> &[String];

    /// Get session ID for database operations
    fn session_id(&self) -> Option<i64>;

    /// Get history steps from previous requests in this session
    fn get_history_steps(&self) -> Vec<ChainStep>;

    /// Get the session's TODO list (plan)
    fn get_session_plan(&self) -> Option<crate::domain::todo::TodoList>;
}
