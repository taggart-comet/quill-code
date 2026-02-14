use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("sql/00-initial-schema.sql"))])
}

/// Bridge from legacy meta-table versioning to PRAGMA user_version.
/// Existing DBs already have the full schema, so we mark migration 1 as applied.
pub fn bridge_legacy_version(conn: &Connection) {
    let has_meta = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .is_ok();

    if has_meta {
        let current_uv: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap_or(0);
        if current_uv == 0 {
            conn.pragma_update(None, "user_version", 1).ok();
        }
    }
}
