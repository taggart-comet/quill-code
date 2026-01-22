use rusqlite::{params, Connection};

pub struct MetaRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MetaRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM meta WHERE key = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_last_used_model_id(&self) -> Result<Option<i64>, String> {
        match self.get("last_used_model_id")? {
            Some(value) => {
                let id = value
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    pub fn set_last_used_model_id(&self, model_id: i64) -> Result<(), String> {
        self.set("last_used_model_id", &model_id.to_string())
    }

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
