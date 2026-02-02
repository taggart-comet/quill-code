use super::types::{Permission, PermissionDecision, PermissionScope};
use crate::infrastructure::db::DbPool;
use rusqlite::params;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

pub trait PermissionStore {
    fn create_permission(&self, permission: Permission) -> Result<Permission, StoreError>;
    fn find_permission(
        &self,
        tool: &str,
        project_id: i32,
        command_pattern: &str,
        resource_pattern: &str,
    ) -> Result<Option<Permission>, StoreError>;
}

pub struct SqlitePermissionStore {
    conn: DbPool,
}

impl SqlitePermissionStore {
    pub fn new(conn: DbPool) -> Self {
        Self { conn }
    }

    fn row_to_permission(&self, row: &rusqlite::Row) -> Result<Permission, rusqlite::Error> {
        let decision_str: String = row.get("decision")?;
        let decision = match decision_str.as_str() {
            "allow" => PermissionDecision::AlwaysAllow,
            "deny" => PermissionDecision::AlwaysDeny,
            "ask" => PermissionDecision::Ask,
            "once" => PermissionDecision::AllowOnce,
            _ => PermissionDecision::Ask, // Default to ask on error
        };

        let scope_str: String = row.get("scope")?;
        let scope = match scope_str.as_str() {
            "session" => PermissionScope::Session,
            "project" => PermissionScope::Project,
            "global" => PermissionScope::Global,
            _ => PermissionScope::Session, // Default to session on error
        };

        let created_at_str: String = row.get("created_at")?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(Permission {
            id: Some(row.get("id")?),
            tool_name: row.get("tool_name")?,
            command_pattern: row.get("command_pattern")?,
            resource_pattern: row.get("resource_pattern")?,
            decision,
            scope,
            project_id: row.get("project_id")?,
            created_at,
        })
    }
}

impl PermissionStore for SqlitePermissionStore {
    fn create_permission(&self, permission: Permission) -> Result<Permission, StoreError> {
        let decision_str = match permission.decision {
            PermissionDecision::AlwaysAllow => "allow",
            PermissionDecision::AlwaysDeny => "deny",
            PermissionDecision::Ask => "ask",
            PermissionDecision::AllowOnce => "once",
        };

        let scope_str = match permission.scope {
            PermissionScope::Session => "session",
            PermissionScope::Project => "project",
            PermissionScope::Global => "global",
        };

        let conn = self.conn.get().map_err(|e| {
            StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to get connection: {}", e),
                ),
            )))
        })?;

        conn.execute(
            "INSERT INTO permissions (
                tool_name, command_pattern, resource_pattern, decision, scope,
                project_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                permission.tool_name,
                permission.command_pattern,
                permission.resource_pattern,
                decision_str.to_string(),
                scope_str.to_string(),
                permission.project_id,
                permission.created_at.to_rfc3339(),
            ],
        )?;

        let id = conn.last_insert_rowid() as i32;
        let mut created = permission;
        created.id = Some(id);
        Ok(created)
    }

    fn find_permission(
        &self,
        tool: &str,
        project_id: i32,
        command_pattern: &str,
        resource_pattern: &str,
    ) -> Result<Option<Permission>, StoreError> {
        // Build query dynamically to handle NULL values properly
        // In SQL, NULL != '', so we need to use IS NULL for empty strings
        let command_clause = if command_pattern.is_empty() {
            "command_pattern IS NULL"
        } else {
            "command_pattern = ?3"
        };

        let resource_clause = if resource_pattern.is_empty() {
            "resource_pattern IS NULL"
        } else {
            "resource_pattern = ?4"
        };

        let query = format!(
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
            project_id, created_at
            FROM permissions
            WHERE project_id = ?1 AND tool_name = ?2 AND {} AND {}
            ORDER BY scope DESC LIMIT 1",
            command_clause, resource_clause
        );

        let conn = self.conn.get().map_err(|e| {
            StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to get connection: {}", e),
                ),
            )))
        })?;

        // Build params based on whether patterns are empty
        let result = if command_pattern.is_empty() && resource_pattern.is_empty() {
            conn.query_row(
                &query,
                params![project_id, tool],
                |row| self.row_to_permission(row),
            )
        } else if command_pattern.is_empty() {
            conn.query_row(
                &query,
                params![project_id, tool, resource_pattern],
                |row| self.row_to_permission(row),
            )
        } else if resource_pattern.is_empty() {
            conn.query_row(
                &query,
                params![project_id, tool, command_pattern],
                |row| self.row_to_permission(row),
            )
        } else {
            conn.query_row(
                &query,
                params![project_id, tool, command_pattern, resource_pattern],
                |row| self.row_to_permission(row),
            )
        };

        match result {
            Ok(permission) => Ok(Some(permission)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Database(e)),
        }
    }
}

impl SqlitePermissionStore {
    fn find_matching_command_permission(
        &self,
        rows: rusqlite::MappedRows<
            impl FnMut(&rusqlite::Row) -> Result<Permission, rusqlite::Error>,
        >,
        tool: &str,
        command: &str,
    ) -> Result<Option<Permission>, StoreError> {
        for row_result in rows {
            match row_result {
                Ok(permission) => {
                    if permission.matches(tool, Some(command), None::<&PathBuf>) {
                        return Ok(Some(permission));
                    }
                }
                Err(e) => return Err(StoreError::Database(e)),
            }
        }

        Ok(None)
    }

    fn find_matching_path_permission(
        &self,
        rows: rusqlite::MappedRows<
            impl FnMut(&rusqlite::Row) -> Result<Permission, rusqlite::Error>,
        >,
        tool: &str,
        path: &PathBuf,
    ) -> Result<Option<Permission>, StoreError> {
        for row_result in rows {
            match row_result {
                Ok(permission) => {
                    if permission.matches(tool, None, Some(path)) {
                        return Ok(Some(permission));
                    }
                }
                Err(e) => return Err(StoreError::Database(e)),
            }
        }

        Ok(None)
    }
}
