use crate::domain::AgentModeType;
use crate::infrastructure::db::DbPool;
use rusqlite::{params, Row};

/// Raw database row for session_requests table
#[derive(Debug, Clone)]
pub struct SessionRequestRow {
    pub id: i64,
    pub _session_id: i64,
    pub _user_settings_id: i64,
    pub prompt: String,
    pub result_summary: Option<String>,
    pub _file_changes: Option<String>,
    pub mode: AgentModeType,
    pub _created_at: String,
}

impl SessionRequestRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        let mode_str: String = row.get(7)?;
        Ok(Self {
            id: row.get(0)?,
            _session_id: row.get(1)?,
            _user_settings_id: row.get(2)?,
            prompt: row.get(3)?,
            result_summary: row.get(4)?,
            _file_changes: row.get(5)?,
            mode: AgentModeType::from_str(&mode_str),
            _created_at: row.get(7)?,
        })
    }
}

pub struct SessionRequestsRepository {
    conn: DbPool,
}

impl SessionRequestsRepository {
    pub fn new(conn: DbPool) -> Self {
        Self { conn }
    }

    /// Create a new session request
    pub fn create(
        &self,
        session_id: i64,
        user_settings_id: i64,
        prompt: &str,
        mode: AgentModeType,
    ) -> Result<SessionRequestRow, String> {
        let created_at = chrono_now();
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;

        conn.execute(
                "INSERT INTO session_requests (session_id, user_settings_id, prompt, mode, created_at) VALUES (?, ?, ?, ?, ?)",
                params![session_id, user_settings_id, prompt, mode.as_str(), created_at],
            )
            .map_err(|e| e.to_string())?;

        let id = conn.last_insert_rowid();

        Ok(SessionRequestRow {
            id,
            _session_id: session_id,
            _user_settings_id: user_settings_id,
            prompt: prompt.to_string(),
            result_summary: None,
            _file_changes: None,
            mode,
            _created_at: created_at,
        })
    }

    /// Update the result summary for a request
    pub fn update_result(&self, request_id: i64, result_summary: &str) -> Result<(), String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;
        conn.execute(
            "UPDATE session_requests SET result_summary = ? WHERE id = ?",
            params![result_summary, request_id],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Update the file changes for a request
    pub fn update_file_changes(&self, request_id: i64, file_changes: &str) -> Result<(), String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;
        conn.execute(
            "UPDATE session_requests SET file_changes = ? WHERE id = ?",
            params![file_changes, request_id],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Find all requests for a session, ordered by creation time
    pub fn find_by_session(&self, session_id: i64) -> Result<Vec<SessionRequestRow>, String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT id, session_id, user_settings_id, prompt, result_summary, file_changes, mode, created_at FROM session_requests WHERE session_id = ? ORDER BY created_at ASC")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![session_id], SessionRequestRow::from_row)
            .map_err(|e| e.to_string())?;

        let mut requests = Vec::new();
        for row in rows {
            requests.push(row.map_err(|e| e.to_string())?);
        }

        Ok(requests)
    }
}

// Helper function to get current timestamp as string
fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}