use super::session_request::SessionRequest;
use std::path::Path;

/// Trait for request context used by workflow execution.
/// This abstraction allows workflow to work with different request implementations
/// without being tightly coupled to the Session entity.
pub trait Request {
    /// Get the history of previous requests
    fn history(&self) -> &[SessionRequest];

    /// Get the current request prompt
    fn current_request(&self) -> &str;

    /// Get the project root path
    fn project_root(&self) -> &Path;

    /// Get the current user settings context
    fn user_settings(&self) -> Option<&crate::domain::UserSettings>;

    /// Get the project ID for permission checks
    fn project_id(&self) -> Option<i32>;

    /// Store a final message produced by the workflow
    fn set_final_message(&mut self, message: String);
}

 
