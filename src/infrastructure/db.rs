use directories::ProjectDirs;
use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Resolves the OS-specific data directory for the application.
fn get_data_dir(app_name: &str) -> Result<PathBuf, String> {
    ProjectDirs::from("", "", app_name)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| "Failed to resolve application data directory".to_string())
}

/// Initializes the SQLite database, creating it if necessary.
/// Returns an open Connection or exits with an error.
pub fn init_db(app_name: &str) -> Result<Arc<Connection>, String> {
    let data_dir = get_data_dir(app_name)?;

    // Ensure parent directories exist
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data directory: {}", e))?;

    let db_path = data_dir.join("agent.db");
    let db_exists = db_path.exists();

    // Open or create database
    let conn = Connection::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    if db_exists {
        log::info!("Database opened: {}", db_path.display());
    } else {
        log::info!("Database created: {}", db_path.display());
    }

    // Set pragmas
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;",
    )
    .map_err(|e| format!("Failed to set pragmas: {}", e))?;

    // Create tables
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            project_root TEXT,
            created_at TEXT NOT NULL,
            session_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id)
        );

        CREATE TABLE IF NOT EXISTS session_requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            user_settings_id INTEGER NOT NULL,
            prompt TEXT NOT NULL,
            result_summary TEXT,
            steps_log TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id),
            FOREIGN KEY (user_settings_id) REFERENCES user_settings(id)
        );

        CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            type TEXT NOT NULL,
            api_key TEXT,
            gguf_file_path TEXT,
            date_added TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS user_settings (
            id INTEGER PRIMARY KEY,
            openai_api_key TEXT,
            openai_tracing_enabled INTEGER NOT NULL DEFAULT 0,
            use_behavior_trees INTEGER NOT NULL DEFAULT 0,
            current_model_id INTEGER,
            FOREIGN KEY (current_model_id) REFERENCES models(id)
        );",
    )
    .map_err(|e| format!("Failed to create tables: {}", e))?;

    // Get current schema version
    let current_version: i32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1); // Default to version 1 if not found

    // Run migrations if needed
    if current_version < 2 {
        log::info!("Migrating database from version {} to 2", current_version);

        // Migration to version 2: add steps_log column
        // Check if column already exists by querying table info
        let column_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('session_requests') WHERE name = 'steps_log'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !column_exists {
            log::info!("Adding steps_log column to session_requests table");
            conn.execute("ALTER TABLE session_requests ADD COLUMN steps_log TEXT", [])
                .map_err(|e| format!("Failed to add steps_log column: {}", e))?;
        }

        // Update schema version
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 2 completed");
    }

    // Migration to version 3: add project_root column to projects
    if current_version < 3 {
        log::info!("Migrating database from version {} to 3", current_version);

        let column_exists = conn
            .prepare("SELECT name FROM pragma_table_info('projects') WHERE name = 'project_root'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !column_exists {
            log::info!("Adding project_root column to projects table");
            conn.execute("ALTER TABLE projects ADD COLUMN project_root TEXT", [])
                .map_err(|e| format!("Failed to add project_root column: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '3')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 3 completed");
    }

    // Migration to version 4: add models table (old schema)
    if current_version < 4 {
        log::info!("Migrating database from version {} to 4", current_version);

        // Check if models table exists
        let table_exists = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='models'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !table_exists {
            log::info!("Creating models table");
            conn.execute(
                "CREATE TABLE models (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    type TEXT NOT NULL,
                    api_key TEXT,
                    gguf_file_path TEXT,
                    date_added TEXT NOT NULL
                )",
                [],
            )
            .map_err(|e| format!("Failed to create models table: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '4')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 4 completed");
    }

    // Migration to version 5: update models table schema
    if current_version < 5 {
        log::info!("Migrating database from version {} to 5", current_version);

        // Check if old columns exist (name, model_type, model_id, created_at)
        let has_old_schema = conn
            .prepare("SELECT name FROM pragma_table_info('models') WHERE name IN ('name', 'model_type', 'model_id', 'created_at')")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if has_old_schema {
            log::info!("Migrating models table to new schema");
            // Drop and recreate the table with new schema
            conn.execute("DROP TABLE IF EXISTS models", [])
                .map_err(|e| format!("Failed to drop old models table: {}", e))?;

            conn.execute(
                "CREATE TABLE models (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    type TEXT NOT NULL,
                    api_key TEXT,
                    gguf_file_path TEXT,
                    date_added TEXT NOT NULL
                )",
                [],
            )
            .map_err(|e| format!("Failed to create new models table: {}", e))?;
        } else {
            // Table might not exist or already has new schema, ensure it exists
            let table_exists = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='models'")
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| row.get::<_, String>(0))
                        .map(|mut rows| rows.next().is_some())
                })
                .unwrap_or(false);

            if !table_exists {
                log::info!("Creating models table with new schema");
                conn.execute(
                    "CREATE TABLE models (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        type TEXT NOT NULL,
                        api_key TEXT,
                        gguf_file_path TEXT,
                        date_added TEXT NOT NULL
                    )",
                    [],
                )
                .map_err(|e| format!("Failed to create models table: {}", e))?;
            }
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '5')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 5 completed");
    }

    // Migration to version 6: add model_name column to models table
    if current_version < 6 {
        log::info!("Migrating database from version {} to 6", current_version);

        let column_exists = conn
            .prepare("SELECT name FROM pragma_table_info('models') WHERE name = 'model_name'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !column_exists {
            log::info!("Adding model_name column to models table");
            conn.execute("ALTER TABLE models ADD COLUMN model_name TEXT", [])
                .map_err(|e| format!("Failed to add model_name column: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '6')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 6 completed");
    }

    // Migration to version 7: add permissions table
    if current_version < 7 {
        log::info!("Migrating database from version {} to 7", current_version);

        // Check if permissions table exists
        let table_exists = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='permissions'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !table_exists {
            log::info!("Creating permissions table");
            conn.execute(
                "CREATE TABLE permissions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    tool_name TEXT NOT NULL,
                    command_pattern TEXT,
                    resource_pattern TEXT,
                    decision TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    project_id INTEGER,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (project_id) REFERENCES projects(id)
                )",
                [],
            )
            .map_err(|e| format!("Failed to create permissions table: {}", e))?;

            // Create indexes for better performance
            conn.execute(
                "CREATE INDEX idx_permissions_tool ON permissions(tool_name)",
                [],
            )
            .map_err(|e| format!("Failed to create permissions tool index: {}", e))?;
            conn.execute(
                "CREATE INDEX idx_permissions_project ON permissions(project_id)",
                [],
            )
            .map_err(|e| format!("Failed to create permissions project index: {}", e))?;
            conn.execute(
                "CREATE INDEX idx_permissions_scope ON permissions(scope)",
                [],
            )
            .map_err(|e| format!("Failed to create permissions scope index: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '7')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 7 completed");
    }

    // Migration to version 8: add user_settings table (legacy key/value schema)
    if current_version < 8 {
        log::info!("Migrating database from version {} to 8", current_version);

        let table_exists = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='user_settings'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !table_exists {
            log::info!("Creating user_settings table");
            conn.execute(
                "CREATE TABLE user_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )",
                [],
            )
            .map_err(|e| format!("Failed to create user_settings table: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '8')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 8 completed");
    }

    // Migration to version 9: user_settings row schema + session_requests.user_settings_id
    if current_version < 9 {
        log::info!("Migrating database from version {} to 9", current_version);

        let has_key_column = conn
            .prepare("SELECT name FROM pragma_table_info('user_settings') WHERE name = 'key'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if has_key_column {
            conn.execute(
                "CREATE TABLE user_settings_new (
                    id INTEGER PRIMARY KEY,
                    openai_api_key TEXT,
                    openai_tracing_enabled INTEGER NOT NULL DEFAULT 0,
                    use_behavior_trees INTEGER NOT NULL DEFAULT 0,
                    current_model_id INTEGER,
                    FOREIGN KEY (current_model_id) REFERENCES models(id)
                )",
                [],
            )
            .map_err(|e| format!("Failed to create new user_settings table: {}", e))?;

            conn.execute(
                "INSERT INTO user_settings_new (id, openai_api_key, openai_tracing_enabled, use_behavior_trees, current_model_id)
                 VALUES (
                    1,
                    (SELECT value FROM user_settings WHERE key = 'openai_api_key'),
                    CASE (SELECT value FROM user_settings WHERE key = 'openai_tracing_enabled') WHEN 'true' THEN 1 ELSE 0 END,
                    CASE (SELECT value FROM user_settings WHERE key = 'use_behavior_trees') WHEN 'true' THEN 1 ELSE 0 END,
                    (SELECT value FROM user_settings WHERE key = 'current_model_id')
                 )",
                [],
            )
            .map_err(|e| format!("Failed to migrate user_settings data: {}", e))?;

            conn.execute("DROP TABLE user_settings", [])
                .map_err(|e| format!("Failed to drop old user_settings table: {}", e))?;
            conn.execute("ALTER TABLE user_settings_new RENAME TO user_settings", [])
                .map_err(|e| format!("Failed to rename user_settings table: {}", e))?;
        } else {
            let table_exists = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name='user_settings'",
                )
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| row.get::<_, String>(0))
                        .map(|mut rows| rows.next().is_some())
                })
                .unwrap_or(false);

            if !table_exists {
                conn.execute(
                    "CREATE TABLE user_settings (
                        id INTEGER PRIMARY KEY,
                        openai_api_key TEXT,
                        openai_tracing_enabled INTEGER NOT NULL DEFAULT 0,
                        use_behavior_trees INTEGER NOT NULL DEFAULT 0,
                        current_model_id INTEGER,
                        FOREIGN KEY (current_model_id) REFERENCES models(id)
                    )",
                    [],
                )
                .map_err(|e| format!("Failed to create user_settings table: {}", e))?;
            }
        }

        let column_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('session_requests') WHERE name = 'user_settings_id'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !column_exists {
            conn.execute(
                "ALTER TABLE session_requests ADD COLUMN user_settings_id INTEGER",
                [],
            )
            .map_err(|e| format!("Failed to add user_settings_id column: {}", e))?;
        }

        conn.execute(
            "INSERT OR IGNORE INTO user_settings (id, openai_tracing_enabled, use_behavior_trees) VALUES (1, 0, 0)",
            [],
        )
        .map_err(|e| format!("Failed to seed user_settings row: {}", e))?;

        let existing_api_key: Option<String> = conn
            .query_row(
                "SELECT api_key FROM models WHERE api_key IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(api_key) = existing_api_key {
            conn.execute(
                "UPDATE user_settings SET openai_api_key = ? WHERE id = 1 AND (openai_api_key IS NULL OR openai_api_key = '')",
                params![api_key],
            )
            .map_err(|e| format!("Failed to migrate OpenAI API key: {}", e))?;
        }

        conn.execute(
            "UPDATE session_requests SET user_settings_id = 1 WHERE user_settings_id IS NULL",
            [],
        )
        .map_err(|e| {
            format!(
                "Failed to backfill session_requests user_settings_id: {}",
                e
            )
        })?;

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '9')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 9 completed");
    }

    // Migration to version 10: add file_changes column to session_requests
    if current_version < 10 {
        log::info!("Migrating database from version {} to 10", current_version);

        // Check if column already exists
        let column_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('session_requests') WHERE name = 'file_changes'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !column_exists {
            log::info!("Adding file_changes column to session_requests table");
            conn.execute(
                "ALTER TABLE session_requests ADD COLUMN file_changes TEXT",
                [],
            )
            .map_err(|e| format!("Failed to add file_changes column: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '10')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 10 completed");
    }

    // Migration to version 11: add web_search_enabled and brave_api_key to user_settings
    if current_version < 11 {
        log::info!("Migrating database from version {} to 11", current_version);

        // Check if columns already exist
        let web_search_enabled_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('user_settings') WHERE name = 'web_search_enabled'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        let brave_api_key_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('user_settings') WHERE name = 'brave_api_key'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if !web_search_enabled_exists {
            log::info!("Adding web_search_enabled column to user_settings table");
            conn.execute(
                "ALTER TABLE user_settings ADD COLUMN web_search_enabled INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| format!("Failed to add web_search_enabled column: {}", e))?;
        }

        if !brave_api_key_exists {
            log::info!("Adding brave_api_key column to user_settings table");
            conn.execute(
                "ALTER TABLE user_settings ADD COLUMN brave_api_key TEXT",
                [],
            )
            .map_err(|e| format!("Failed to add brave_api_key column: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '11')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 11 completed");
    }

    // Migration to version 12: rename path_pattern to resource_pattern in permissions table
    if current_version < 12 {
        log::info!("Migrating database from version {} to 12", current_version);

        // Check if path_pattern column exists and resource_pattern doesn't
        let path_pattern_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('permissions') WHERE name = 'path_pattern'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        let resource_pattern_exists = conn
            .prepare(
                "SELECT name FROM pragma_table_info('permissions') WHERE name = 'resource_pattern'",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);

        if path_pattern_exists && !resource_pattern_exists {
            log::info!("Renaming path_pattern to resource_pattern in permissions table");
            conn.execute(
                "ALTER TABLE permissions RENAME COLUMN path_pattern TO resource_pattern",
                [],
            )
            .map_err(|e| format!("Failed to rename path_pattern column: {}", e))?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '12')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;

        log::info!("Database migration to version 12 completed");
    }

    conn.execute(
        "INSERT OR IGNORE INTO user_settings (id, openai_tracing_enabled, use_behavior_trees, web_search_enabled) VALUES (1, 0, 0, 0)",
        [],
    )
    .map_err(|e| format!("Failed to ensure user_settings row: {}", e))?;

    Ok(Arc::new(conn))
}
