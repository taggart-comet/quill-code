use super::session_request::SessionRequest;
use std::path::{Path, PathBuf};

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

    /// Store a final message produced by the workflow
    fn set_final_message(&mut self, message: String);
}

/// A virtual request implementation that can be created without a full Session entity.
/// Useful for testing or when you need to create a request with just a prompt and project root.
pub struct VirtualRequest {
    prompt: String,
    project_root: PathBuf,
    history: Vec<SessionRequest>,
    final_message: Option<String>,
}

impl VirtualRequest {
    /// Create a new virtual request with the given prompt and project root.
    /// The history will be empty by default.
    pub fn new(prompt: impl Into<String>, project_root: impl Into<PathBuf>) -> Self {
        Self {
            prompt: prompt.into(),
            project_root: project_root.into(),
            history: Vec::new(),
            final_message: None,
        }
    }
}

impl Request for VirtualRequest {
    fn history(&self) -> &[SessionRequest] {
        &self.history
    }

    fn current_request(&self) -> &str {
        &self.prompt
    }

    fn project_root(&self) -> &Path {
        &self.project_root
    }

    fn set_final_message(&mut self, message: String) {
        self.final_message = Some(message);
    }
}
