use super::session_request::SessionRequest;
use crate::repository::SessionRow;

/// Domain entity representing a conversation session.
/// A session belongs to a project and contains a named conversation history.
#[derive(Debug, Clone)]
pub struct Session {
    id: i64,
    project_id: i64,
    name: String,
    created_at: u64,
    requests: Vec<SessionRequest>,
    current_request: String,
}

impl Session {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn project_id(&self) -> i64 {
        self.project_id
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
        requests: Vec<SessionRequest>
    ) -> Self {
        let mut session = Self::from(session_row);
        session.set_requests(requests);
        session
    }
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            name: row.name,
            created_at: row.created_at.parse().unwrap_or(0),
            requests: Vec::new(),
            current_request: String::new(),
        }
    }
}
