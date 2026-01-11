use rusqlite::{Connection, params, Row};

/// Raw database row for session_requests table
#[derive(Debug, Clone)]
pub struct SessionRequestRow {
    pub id: i64,
    pub session_id: i64,
    pub prompt: String,
    pub result_summary: Option<String>,
    pub steps_log: Option<String>,
    pub created_at: String,
}

impl SessionRequestRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            prompt: row.get(2)?,
            result_summary: row.get(3)?,
            steps_log: row.get(4)?,
            created_at: row.get(5)?,
        })
    }
}

pub struct SessionRequestsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SessionRequestsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create a new session request
    pub fn create(&self, session_id: i64, prompt: &str) -> Result<SessionRequestRow, String> {
        let created_at = chrono_now();

        self.conn
            .execute(
                "INSERT INTO session_requests (session_id, prompt, created_at) VALUES (?, ?, ?)",
                params![session_id, prompt, created_at],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(SessionRequestRow {
            id,
            session_id,
            prompt: prompt.to_string(),
            result_summary: None,
            steps_log: None,
            created_at,
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

    /// Find a request by ID
    pub fn find_by_id(&self, id: i64) -> Result<Option<SessionRequestRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, prompt, result_summary, steps_log, created_at FROM session_requests WHERE id = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![id], SessionRequestRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    /// Find all requests for a session, ordered by creation time
    pub fn find_by_session(&self, session_id: i64) -> Result<Vec<SessionRequestRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, prompt, result_summary, steps_log, created_at FROM session_requests WHERE session_id = ? ORDER BY created_at ASC")
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

    /// Get the latest request for a session (most recent)
    pub fn find_latest_by_session(&self, session_id: i64) -> Result<Option<SessionRequestRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, prompt, result_summary, steps_log, created_at FROM session_requests WHERE session_id = ? ORDER BY created_at DESC LIMIT 1")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![session_id], SessionRequestRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    /// Delete all requests for a session
    pub fn delete_by_session(&self, session_id: i64) -> Result<usize, String> {
        let affected = self
            .conn
            .execute("DELETE FROM session_requests WHERE session_id = ?", params![session_id])
            .map_err(|e| e.to_string())?;

        Ok(affected)
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

// Extension trait for Option<T> to handle database results
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        
        // Create the tables
        conn.execute_batch(
            "CREATE TABLE sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE session_requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                prompt TEXT NOT NULL,
                result_summary TEXT,
                steps_log TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );"
        ).unwrap();

        // Insert a test session
        conn.execute(
            "INSERT INTO sessions (project_id, name, created_at) VALUES (1, 'Test Session', '1234567890')",
            []
        ).unwrap();

        conn
    }

    #[test]
    fn test_create_request() {
        let conn = setup_test_db();
        let repo = SessionRequestsRepository::new(&conn);

        let request = repo.create(1, "Test prompt").unwrap();
        assert_eq!(request.session_id, 1);
        assert_eq!(request.prompt, "Test prompt");
        assert!(request.result_summary.is_none());
    }

    #[test]
    fn test_update_result() {
        let conn = setup_test_db();
        let repo = SessionRequestsRepository::new(&conn);

        let request = repo.create(1, "Test prompt").unwrap();
        repo.update_result(request.id, "Test result").unwrap();

        let updated = repo.find_by_id(request.id).unwrap().unwrap();
        assert_eq!(updated.result_summary, Some("Test result".to_string()));
    }

    #[test]
    fn test_find_by_session() {
        let conn = setup_test_db();
        let repo = SessionRequestsRepository::new(&conn);

        repo.create(1, "First prompt").unwrap();
        repo.create(1, "Second prompt").unwrap();

        let requests = repo.find_by_session(1).unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].prompt, "First prompt");
        assert_eq!(requests[1].prompt, "Second prompt");
    }
}
