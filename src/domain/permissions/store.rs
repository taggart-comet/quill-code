use super::types::{Permission, PermissionDecision, PermissionScope};
use crate::infrastructure::db::DbPool;
use rusqlite::params;
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
        let mut param_num = 3;
        let command_clause = if command_pattern.is_empty() {
            "command_pattern IS NULL".to_string()
        } else {
            let clause = format!("command_pattern = ?{}", param_num);
            param_num += 1;
            clause
        };

        let resource_clause = if resource_pattern.is_empty() {
            "resource_pattern IS NULL".to_string()
        } else {
            format!("resource_pattern = ?{}", param_num)
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

        // Build params list dynamically to match the query
        let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(project_id),
            Box::new(tool.to_string()),
        ];

        if !command_pattern.is_empty() {
            param_values.push(Box::new(command_pattern.to_string()));
        }

        if !resource_pattern.is_empty() {
            param_values.push(Box::new(resource_pattern.to_string()));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = param_values
            .iter()
            .map(|p| p.as_ref())
            .collect();

        let result = conn.query_row(&query, params_refs.as_slice(), |row| {
            self.row_to_permission(row)
        });

        match result {
            Ok(permission) => Ok(Some(permission)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Database(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2_sqlite::SqliteConnectionManager;

    fn setup_test_db() -> DbPool {
        let manager = SqliteConnectionManager::memory();
        let pool = r2d2::Pool::new(manager).unwrap();
        let conn = pool.get().unwrap();

        // Create permissions table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS permissions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name TEXT NOT NULL,
                command_pattern TEXT,
                resource_pattern TEXT,
                decision TEXT NOT NULL,
                scope TEXT NOT NULL,
                project_id INTEGER,
                created_at TEXT NOT NULL
            )",
            [],
        ).unwrap();

        pool
    }

    #[test]
    fn test_find_permission_with_null_patterns() {
        let pool = setup_test_db();
        let store = SqlitePermissionStore::new(pool.clone());

        // Create permission with NULL command and resource patterns
        let permission = Permission::new(
            "test_tool".to_string(),
            None,
            None,
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        store.create_permission(permission).unwrap();

        // Should find the permission when searching with empty strings
        let result = store.find_permission("test_tool", 1, "", "").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().decision, PermissionDecision::AlwaysAllow);
    }

    #[test]
    fn test_find_permission_with_command_only() {
        let pool = setup_test_db();
        let store = SqlitePermissionStore::new(pool.clone());

        // Create permission with command but NULL resource
        let permission = Permission::new(
            "test_tool".to_string(),
            Some("echo test".to_string()),
            None,
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        store.create_permission(permission).unwrap();

        // Should find the permission when searching with command and empty resource
        let result = store.find_permission("test_tool", 1, "echo test", "").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().decision, PermissionDecision::AlwaysDeny);
    }

    #[test]
    fn test_find_permission_with_resource_only() {
        let pool = setup_test_db();
        let store = SqlitePermissionStore::new(pool.clone());

        // Create permission with resource but NULL command
        let permission = Permission::new(
            "test_tool".to_string(),
            None,
            Some("/path/to/file".to_string()),
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        store.create_permission(permission).unwrap();

        // Should find the permission when searching with empty command and resource
        let result = store.find_permission("test_tool", 1, "", "/path/to/file").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().decision, PermissionDecision::AlwaysAllow);
    }

    #[test]
    fn test_find_permission_with_both_patterns() {
        let pool = setup_test_db();
        let store = SqlitePermissionStore::new(pool.clone());

        // Create permission with both patterns
        let permission = Permission::new(
            "test_tool".to_string(),
            Some("rm -rf".to_string()),
            Some("/etc/passwd".to_string()),
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        store.create_permission(permission).unwrap();

        // Should find the permission when searching with both patterns
        let result = store.find_permission("test_tool", 1, "rm -rf", "/etc/passwd").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().decision, PermissionDecision::AlwaysDeny);
    }

    #[test]
    fn test_find_permission_no_match() {
        let pool = setup_test_db();
        let store = SqlitePermissionStore::new(pool.clone());

        // Create permission with command
        let permission = Permission::new(
            "test_tool".to_string(),
            Some("echo test".to_string()),
            None,
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        store.create_permission(permission).unwrap();

        // Should NOT find when searching with different command
        let result = store.find_permission("test_tool", 1, "rm -rf", "").unwrap();
        assert!(result.is_none());
    }
}