use rusqlite::{params, Connection, Row};

/// Raw database row for sessions table
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub created_at: String,
}

impl SessionRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            project_id: row.get(1)?,
            name: row.get(2)?,
            created_at: row.get(3)?,
        })
    }
}

pub struct SessionsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SessionsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, project_id: i64, name: &str) -> Result<SessionRow, String> {
        let created_at = chrono_now();

        self.conn
            .execute(
                "INSERT INTO sessions (project_id, name, created_at) VALUES (?, ?, ?)",
                params![project_id, name, created_at],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(SessionRow {
            id,
            project_id,
            name: name.to_string(),
            created_at,
        })
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<SessionRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, project_id, name, created_at FROM sessions WHERE id = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![id], SessionRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{}", secs)
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
