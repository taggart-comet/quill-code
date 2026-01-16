use rusqlite::{params, Connection, Row};

/// Raw database row for projects table
#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub project_root: Option<String>,
    pub created_at: String,
    pub session_count: i64,
}

impl ProjectRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            project_root: row.get(2)?,
            created_at: row.get(3)?,
            session_count: row.get(4)?,
        })
    }
}

pub struct ProjectsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ProjectsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<ProjectRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, project_root, created_at, session_count FROM projects WHERE id = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![id], ProjectRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    pub fn find_by_name(&self, name: &str) -> Result<Option<ProjectRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, project_root, created_at, session_count FROM projects WHERE name = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![name], ProjectRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    pub fn create(&self, name: &str, project_root: &str) -> Result<ProjectRow, String> {
        let created_at = chrono_now();

        self.conn
            .execute(
                "INSERT INTO projects (name, project_root, created_at, session_count) VALUES (?, ?, ?, 0)",
                params![name, project_root, created_at],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(ProjectRow {
            id,
            name: name.to_string(),
            project_root: Some(project_root.to_string()),
            created_at,
            session_count: 0,
        })
    }

    pub fn increment_session_count(&self, project_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE projects SET session_count = session_count + 1 WHERE id = ?",
                params![project_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_or_create(
        &self,
        name: &str,
        project_root: &str,
    ) -> Result<(ProjectRow, bool), String> {
        if let Some(row) = self.find_by_name(name)? {
            Ok((row, false))
        } else {
            let row = self.create(name, project_root)?;
            Ok((row, true))
        }
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // ISO 8601 format approximation
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
