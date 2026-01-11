use directories::ProjectDirs;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

/// Resolves the OS-specific data directory for the application.
fn get_data_dir(app_name: &str) -> Result<PathBuf, String> {
    ProjectDirs::from("", "", app_name)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| "Failed to resolve application data directory".to_string())
}

/// Initializes the SQLite database, creating it if necessary.
/// Returns an open Connection or exits with an error.
pub fn init_db(app_name: &str, _debug: bool) -> Result<Connection, String> {
    let data_dir = get_data_dir(app_name)?;

    // Ensure parent directories exist
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data directory: {}", e))?;

    let db_path = data_dir.join("agent.db");
    let db_exists = db_path.exists();

    // Open or create database
    let conn =
        Connection::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

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
            .prepare("SELECT name FROM pragma_table_info('session_requests') WHERE name = 'steps_log'")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|mut rows| rows.next().is_some())
            })
            .unwrap_or(false);
        
        if !column_exists {
            log::info!("Adding steps_log column to session_requests table");
            conn.execute(
                "ALTER TABLE session_requests ADD COLUMN steps_log TEXT",
                [],
            )
            .map_err(|e| format!("Failed to add steps_log column: {}", e))?;
        }
        
        // Update schema version
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2')",
            [],
        )
        .map_err(|e| format!("Failed to update schema_version: {}", e))?;
        
        log::info!("Database migration completed");
    } else {
        // Insert schema_version if missing (for new databases)
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '2')",
            [],
        )
        .map_err(|e| format!("Failed to insert schema_version: {}", e))?;
    }

    Ok(conn)
}
