use crate::domain::AgentModeType;
use crate::repository::SessionRequestRow;

/// Domain entity representing a user request within a session.
/// Each request contains the user's prompt and the resulting summary.
#[derive(Debug, Clone)]
pub struct SessionRequest {
    prompt: String,
    result_summary: Option<String>,
    #[allow(dead_code)]
    mode: AgentModeType,
}

impl SessionRequest {
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn result_summary(&self) -> Option<&str> {
        self.result_summary.as_deref()
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> AgentModeType {
        self.mode
    }

    pub fn from_row(row: SessionRequestRow) -> Self {
        Self {
            prompt: row.prompt,
            result_summary: row.result_summary,
            mode: row.mode,
        }
    }
}
