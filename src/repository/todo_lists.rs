#![allow(dead_code)]

use rusqlite::{params, Connection, Row};

/// Raw database row for todo_lists table
#[derive(Debug, Clone)]
pub struct TodoListRow {
    pub id: i64,
    pub session_id: i64,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

impl TodoListRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            content: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }
}

pub struct TodoListRepository<'a> {
    conn: &'a Connection,
}

impl<'a> TodoListRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Gets existing TODO list for session or creates a new one
    pub fn get_or_create_for_session(&self, session_id: i64) -> Result<TodoListRow, String> {
        // Try to get existing list
        if let Some(list) = self.get_by_session(session_id)? {
            return Ok(list);
        }

        // Create new list
        let now = chrono_now();
        let content = default_content();
        self.conn
            .execute(
                "INSERT INTO todo_lists (session_id, content, created_at, updated_at) VALUES (?, ?, ?, ?)",
                params![session_id, content, now, now],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(TodoListRow {
            id,
            session_id,
            content,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Gets TODO list for a session
    pub fn get_by_session(&self, session_id: i64) -> Result<Option<TodoListRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, content, created_at, updated_at FROM todo_lists WHERE session_id = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![session_id], TodoListRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    /// Updates the JSON content for a TODO list
    pub fn update_content(&self, todo_list_id: i64, content: &str) -> Result<(), String> {
        let now = chrono_now();
        self.conn
            .execute(
                "UPDATE todo_lists SET content = ?, updated_at = ? WHERE id = ?",
                params![content, now, todo_list_id],
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

fn default_content() -> String {
    "{\"items\":[]}".to_string()
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
