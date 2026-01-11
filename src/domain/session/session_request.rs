use crate::repository::SessionRequestRow;

/// Domain entity representing a user request within a session.
/// Each request contains the user's prompt and the resulting summary.
#[derive(Debug, Clone)]
pub struct SessionRequest {
    id: i64,
    session_id: i64,
    prompt: String,
    result_summary: Option<String>,
    created_at: u64,
}

impl SessionRequest {
    pub fn new(session_id: i64, prompt: String) -> Self {
        Self {
            id: 0, // Will be set when saved to database
            session_id,
            prompt,
            result_summary: None,
            created_at: 0, // Will be set when saved to database
        }
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn result_summary(&self) -> Option<&str> {
        self.result_summary.as_deref()
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    pub fn set_result_summary(&mut self, summary: String) {
        self.result_summary = Some(summary);
    }

    pub fn has_result(&self) -> bool {
        self.result_summary.is_some()
    }
}

impl From<SessionRequestRow> for SessionRequest {
    fn from(row: SessionRequestRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            prompt: row.prompt,
            result_summary: row.result_summary,
            created_at: row.created_at.parse().unwrap_or(0),
        }
    }
}
