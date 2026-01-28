use crate::domain::ModelType;
use rusqlite::{params, Connection, Row};

/// Raw database row for models table
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub id: i64,
    pub model_type: ModelType,
    pub _api_key: Option<String>,
    pub gguf_file_path: Option<String>,
    pub model_name: Option<String>,
    pub _date_added: String,
}

impl ModelRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        let model_type_str: String = row.get(1)?;
        let model_type = ModelType::from_str(&model_type_str).unwrap_or(ModelType::Local);

        Ok(Self {
            id: row.get(0)?,
            model_type,
            _api_key: row.get(2)?,
            gguf_file_path: row.get(3)?,
            model_name: row.get(4)?,
            _date_added: row.get(5)?,
        })
    }
}

pub struct ModelsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ModelsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<ModelRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, type, api_key, gguf_file_path, model_name, date_added FROM models WHERE id = ?")
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![id], ModelRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    pub fn find_by_type(&self, model_type: ModelType) -> Result<Vec<ModelRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, type, api_key, gguf_file_path, model_name, date_added FROM models WHERE type = ? ORDER BY date_added DESC")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![model_type.as_str()], ModelRow::from_row)
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| e.to_string())?);
        }

        Ok(results)
    }

    pub fn create(
        &self,
        model_type: ModelType,
        api_key: Option<&str>,
        gguf_file_path: Option<&str>,
        model_name: Option<&str>,
    ) -> Result<ModelRow, String> {
        let date_added = chrono_now();

        self.conn
            .execute(
                "INSERT INTO models (type, api_key, gguf_file_path, model_name, date_added) VALUES (?, ?, ?, ?, ?)",
                params![model_type.as_str(), api_key, gguf_file_path, model_name, date_added],
            )
            .map_err(|e| e.to_string())?;

        let id = self.conn.last_insert_rowid();

        Ok(ModelRow {
            id,
            model_type,
            _api_key: api_key.map(|s| s.to_string()),
            gguf_file_path: gguf_file_path.map(|s| s.to_string()),
            model_name: model_name.map(|s| s.to_string()),
            _date_added: date_added,
        })
    }

    pub fn update_model_name(&self, id: i64, model_name: Option<&str>) -> Result<bool, String> {
        let rows_affected = self
            .conn
            .execute(
                "UPDATE models SET model_name = ? WHERE id = ?",
                params![model_name, id],
            )
            .map_err(|e| e.to_string())?;

        Ok(rows_affected > 0)
    }
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
