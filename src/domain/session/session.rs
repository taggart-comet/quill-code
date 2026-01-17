use super::{request::Request, session_request::SessionRequest};
use crate::domain::Project;
use crate::repository::SessionRow;
use std::path::Path;

/// Domain entity representing a conversation session.
/// A session belongs to a project and contains a named conversation history.
#[derive(Debug, Clone)]
pub struct Session {
    id: i64,
    project_id: i64,
    project: Project,
    name: String,
    created_at: u64,
    requests: Vec<SessionRequest>,
    current_request: String,
    final_message: Option<String>,
}

impl Session {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    pub fn requests(&self) -> &[SessionRequest] {
        &self.requests
    }

    pub fn add_request(&mut self, request: SessionRequest) {
        self.requests.push(request);
    }

    pub fn set_current_request(&mut self, prompt: String) {
        self.current_request = prompt;
    }

    pub fn current_request(&self) -> &str {
        &self.current_request
    }

    pub fn set_requests(&mut self, requests: Vec<SessionRequest>) {
        self.requests = requests;
    }

    pub fn load_with_requests(
        session_row: crate::repository::SessionRow,
        project: Project,
        requests: Vec<SessionRequest>,
    ) -> Self {
        let mut session = Self::from_row_with_project(session_row, project);
        session.set_requests(requests);
        session
    }

    /// Create a Session from a SessionRow with a Project entity
    pub fn from_row_with_project(row: SessionRow, project: Project) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            project,
            name: row.name,
            created_at: row.created_at.parse().unwrap_or(0),
            requests: Vec::new(),
            current_request: String::new(),
            final_message: None,
        }
    }
}

impl Request for Session {
    fn history(&self) -> &[SessionRequest] {
        &self.requests
    }

    fn current_request(&self) -> &str {
        &self.current_request
    }

    fn project_root(&self) -> &Path {
        self.project.project_root()
    }

    fn set_final_message(&mut self, message: String) {
        self.final_message = Some(message);
    }
}

// Note: From<SessionRow> is no longer implemented because Session requires a Project entity.
// Use Session::from_row_with_project() instead.
