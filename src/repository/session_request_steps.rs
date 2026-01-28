use crate::domain::workflow::step::ChainStep;
use crate::infrastructure::db::DbPool;
use rusqlite::params;

pub struct SessionRequestStepsRepository {
    conn: DbPool,
}

impl SessionRequestStepsRepository {
    pub fn new(conn: DbPool) -> Self {
        Self { conn }
    }

    /// Save steps for a request
    /// Each step is saved with its index in the chain
    pub fn save_steps_for_request(
        &self,
        request_id: i64,
        steps: &[ChainStep],
    ) -> Result<(), String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get database connection: {}", e))?;

        for (index, step) in steps.iter().enumerate() {
            let step_json = serde_json::to_string(step)
                .map_err(|e| format!("Failed to serialize step: {}", e))?;

            conn.execute(
                "INSERT INTO session_request_steps (request_id, step_index, step_json) VALUES (?1, ?2, ?3)",
                params![request_id, index as i32, step_json],
            )
            .map_err(|e| format!("Failed to insert step: {}", e))?;
        }

        Ok(())
    }

    /// Load steps for a single request
    pub fn load_steps_for_request(&self, request_id: i64) -> Result<Vec<ChainStep>, String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get database connection: {}", e))?;

        let mut stmt = conn
            .prepare(
                "SELECT step_json FROM session_request_steps
                 WHERE request_id = ?1
                 ORDER BY step_index ASC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let steps = stmt
            .query_map(params![request_id], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| format!("Failed to query steps: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect steps: {}", e))?;

        let mut result = Vec::new();
        for json in steps {
            let step: ChainStep = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to deserialize step: {}", e))?;
            result.push(step);
        }

        Ok(result)
    }

    /// Load all steps for all requests in a session
    /// Returns steps ordered by request creation time and step index
    /// This provides the complete conversation history for context
    pub fn load_steps_for_session(&self, session_id: i64) -> Result<Vec<ChainStep>, String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get database connection: {}", e))?;

        let mut stmt = conn
            .prepare(
                "SELECT srs.step_json
                 FROM session_request_steps srs
                 JOIN session_requests sr ON srs.request_id = sr.id
                 WHERE sr.session_id = ?1
                 ORDER BY sr.created_at ASC, srs.step_index ASC",
            )
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let steps = stmt
            .query_map(params![session_id], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| format!("Failed to query steps: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect steps: {}", e))?;

        let mut result = Vec::new();
        for json in steps {
            let step: ChainStep = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to deserialize step: {}", e))?;
            result.push(step);
        }

        Ok(result)
    }
}
