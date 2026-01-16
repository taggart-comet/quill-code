use directories::ProjectDirs;
use rusqlite::Connection;
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
pub fn init_db(app_name: &str, _debug: bool) -> Result<Arc<Connection>, String> {
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
            prompt TEXT NOT NULL,
            result_summary TEXT,
            steps_log TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        );

        CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            type TEXT NOT NULL,
            api_key TEXT,
            gguf_file_path TEXT,
            date_added TEXT NOT NULL
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
    } else {
        // Insert schema_version if missing (for new databases)
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '6')",
            [],
        )
        .map_err(|e| format!("Failed to insert schema_version: {}", e))?;
    }

    Ok(Arc::new(conn))
}
