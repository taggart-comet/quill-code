mod migrations;

use directories::ProjectDirs;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Resolves the OS-specific data directory for the application.
fn get_data_dir(app_name: &str) -> Result<PathBuf, String> {
    ProjectDirs::from("", "", app_name)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| "Failed to resolve application data directory".to_string())
}

fn configure_connection(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

/// Initializes the SQLite database, creating it if necessary.
/// Returns an open connection pool for thread-safe sharing.
pub fn init_db(app_name: &str) -> Result<DbPool, String> {
    let data_dir = get_data_dir(app_name)?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data directory: {}", e))?;

    let db_path = data_dir.join("agent.db");
    let db_exists = db_path.exists();

    let manager =
        SqliteConnectionManager::file(&db_path).with_init(|conn| configure_connection(conn));
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(|e| format!("Failed to create database pool: {}", e))?;

    if db_exists {
        log::info!("Database opened: {}", db_path.display());
    } else {
        log::info!("Database created: {}", db_path.display());
    }

    let mut conn = pool
        .get()
        .map_err(|e| format!("Failed to get database connection: {}", e))?;

    // Bridge legacy meta-table versioning before running rusqlite_migration
    migrations::bridge_legacy_version(&conn);

    migrations::migrations()
        .to_latest(&mut conn)
        .map_err(|e| format!("Failed to run database migrations: {}", e))?;

    // Ensure the default user_settings row exists
    conn.execute(
        "INSERT OR IGNORE INTO user_settings (id, openai_tracing_enabled, use_behavior_trees, web_search_enabled, max_tool_calls_per_request) VALUES (1, 0, 0, 0, 50)",
        [],
    )
    .map_err(|e| format!("Failed to ensure user_settings row: {}", e))?;

    Ok(pool)
}
