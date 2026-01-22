use rusqlite::{params, Connection, Row};
use std::sync::Arc;

/// Raw database row for session_requests table
#[derive(Debug, Clone)]
pub struct SessionRequestRow {
    pub id: i64,
    pub _session_id: i64,
    pub _user_settings_id: i64,
    pub prompt: String,
    pub result_summary: Option<String>,
    pub _steps_log: Option<String>,
    pub _file_changes: Option<String>,
    pub _created_at: String,
}

impl SessionRequestRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            _session_id: row.get(1)?,
            _user_settings_id: row.get(2)?,
            prompt: row.get(3)?,
            result_summary: row.get(4)?,
            _steps_log: row.get(5)?,
            _file_changes: row.get(6)?,
            _created_at: row.get(7)?,
        })
    }
}

pub struct SessionRequestsRepository {
    conn: Arc<Connection>,
}

impl SessionRequestsRepository {
    pub fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }

    /// Create a new session request
    pub fn create(
        &self,
        session_id: i64,
        user_settings_id: i64,
        prompt: &str,
    ) -> Result<SessionRequestRow, String> {
        let created_at = chrono_now();

        self.conn
            .execute(
                "INSERT INTO session_requests (session_id, user_settings_id, prompt, created_at) VALUES (?, ?, ?, ?)",
                params![session_id, user_settings_id, prompt, created_at],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(SessionRequestRow {
            id,
            _session_id: session_id,
            _user_settings_id: user_settings_id,
            prompt: prompt.to_string(),
            result_summary: None,
            _steps_log: None,
            _file_changes: None,
            _created_at: created_at,
        })
    }

    /// Update the result summary for a request
    pub fn update_result(&self, request_id: i64, result_summary: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE session_requests SET result_summary = ? WHERE id = ?",
                params![result_summary, request_id],
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Update the steps log for a request
    pub fn update_steps_log(&self, request_id: i64, steps_log: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE session_requests SET steps_log = ? WHERE id = ?",
                params![steps_log, request_id],
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Update the file changes for a request
    pub fn update_file_changes(&self, request_id: i64, file_changes: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE session_requests SET file_changes = ? WHERE id = ?",
                params![file_changes, request_id],
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Find all requests for a session, ordered by creation time
    pub fn find_by_session(&self, session_id: i64) -> Result<Vec<SessionRequestRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, user_settings_id, prompt, result_summary, steps_log, file_changes, created_at FROM session_requests WHERE session_id = ? ORDER BY created_at ASC")
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
