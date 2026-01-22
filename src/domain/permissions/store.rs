use super::types::{Permission, PermissionDecision, PermissionScope};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

pub trait PermissionStore {
    fn create_permission(&self, permission: Permission) -> Result<Permission, StoreError>;
    fn find_tool_permission(
        &self,
        tool: &str,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError>;
    fn find_command_permission(
        &self,
        tool: &str,
        command: &str,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError>;
    fn find_path_permission(
        &self,
        tool: &str,
        path: &PathBuf,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError>;
}

pub struct SqlitePermissionStore {
    conn: Arc<Connection>,
}

impl SqlitePermissionStore {
    pub fn new(conn: Arc<Connection>) -> Self {
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

        self.conn.execute(
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

        let id = self.conn.last_insert_rowid() as i32;
        let mut created = permission;
        created.id = Some(id);
        Ok(created)
    }

    fn find_tool_permission(
        &self,
        tool: &str,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError> {
        let query = if project_id.is_some() {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
             WHERE tool_name = ?1 AND (project_id = ?2 OR project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC, project_id DESC LIMIT 1"
        } else {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
             WHERE tool_name = ?1 AND (project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC LIMIT 1"
        };

        let mut stmt = self.conn.prepare(query)?;
        if let Some(pid) = project_id {
            let mut rows = stmt.query_map(params![tool, pid], |row| self.row_to_permission(row))?;
            return match rows.next() {
                Some(Ok(permission)) => Ok(Some(permission)),
                Some(Err(e)) => Err(StoreError::Database(e)),
                None => Ok(None),
            };
        }

        let mut rows = stmt.query_map(params![tool], |row| self.row_to_permission(row))?;
        match rows.next() {
            Some(Ok(permission)) => Ok(Some(permission)),
            Some(Err(e)) => Err(StoreError::Database(e)),
            None => Ok(None),
        }
    }

    fn find_command_permission(
        &self,
        tool: &str,
        command: &str,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError> {
        let query = if project_id.is_some() {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
             WHERE tool_name = ?1 AND command_pattern IS NOT NULL 
             AND (project_id = ?2 OR project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC, project_id DESC"
        } else {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
             WHERE tool_name = ?1 AND command_pattern IS NOT NULL 
             AND (project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC"
        };

        let mut stmt = self.conn.prepare(query)?;
        if let Some(pid) = project_id {
            let rows = stmt.query_map(params![tool, pid], |row| self.row_to_permission(row))?;
            return self.find_matching_command_permission(rows, tool, command);
        }

        let rows = stmt.query_map(params![tool], |row| self.row_to_permission(row))?;
        self.find_matching_command_permission(rows, tool, command)
    }

    fn find_path_permission(
        &self,
        tool: &str,
        path: &PathBuf,
        project_id: Option<i32>,
    ) -> Result<Option<Permission>, StoreError> {
        let query = if project_id.is_some() {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
              WHERE tool_name = ?1 AND resource_pattern IS NOT NULL
             AND (project_id = ?2 OR project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC, project_id DESC"
        } else {
            "SELECT id, tool_name, command_pattern, resource_pattern, decision, scope,
                    project_id, created_at 
             FROM permissions 
              WHERE tool_name = ?1 AND resource_pattern IS NOT NULL
             AND (project_id IS NULL OR scope = 'global')
             ORDER BY scope DESC"
        };

        let mut stmt = self.conn.prepare(query)?;
        if let Some(pid) = project_id {
            let rows = stmt.query_map(params![tool, pid], |row| self.row_to_permission(row))?;
            return self.find_matching_path_permission(rows, tool, path);
        }

        let rows = stmt.query_map(params![tool], |row| self.row_to_permission(row))?;
        self.find_matching_path_permission(rows, tool, path)
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
