use rusqlite::{params, Connection, Row};

#[derive(Debug, Clone)]
pub struct UserSettingsRow {
    pub id: i64,
    pub openai_api_key: Option<String>,
    pub openai_tracing_enabled: bool,
    pub use_behavior_trees: bool,
    pub current_model_id: Option<i64>,
    pub web_search_enabled: bool,
    pub brave_api_key: Option<String>,
    pub max_tool_calls_per_request: i32,
    pub auth_method: String,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expiry: Option<i64>,
    pub oauth_account_id: Option<String>,
}

impl UserSettingsRow {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            id: row.get(0)?,
            openai_api_key: row.get(1)?,
            openai_tracing_enabled: row.get::<_, i64>(2)? != 0,
            use_behavior_trees: row.get::<_, i64>(3)? != 0,
            current_model_id: row.get(4)?,
            web_search_enabled: row.get::<_, i64>(5)? != 0,
            brave_api_key: row.get(6)?,
            max_tool_calls_per_request: row.get(7)?,
            auth_method: row
                .get::<_, Option<String>>(8)?
                .unwrap_or_else(|| "api_key".to_string()),
            oauth_access_token: row.get(9)?,
            oauth_refresh_token: row.get(10)?,
            oauth_token_expiry: row.get(11)?,
            oauth_account_id: row.get(12)?,
        })
    }
}

pub struct UserSettingsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> UserSettingsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_current(&self) -> Result<UserSettingsRow, String> {
        self.ensure_row()?;
        self.get_by_id(1)?
            .ok_or_else(|| "user_settings row not found".to_string())
    }

    pub fn get_by_id(&self, id: i64) -> Result<Option<UserSettingsRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, openai_api_key, openai_tracing_enabled, use_behavior_trees, current_model_id, web_search_enabled, brave_api_key, max_tool_calls_per_request, auth_method, oauth_access_token, oauth_refresh_token, oauth_token_expiry, oauth_account_id FROM user_settings WHERE id = ?",
            )
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(params![id], UserSettingsRow::from_row)
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    pub fn update_openai_api_key(&self, api_key: Option<&str>) -> Result<(), String> {
        self.ensure_row()?;
        self.conn
            .execute(
                "UPDATE user_settings SET openai_api_key = ?, auth_method = 'api_key' WHERE id = 1",
                params![api_key],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_openai_tracing_enabled(&self, enabled: bool) -> Result<(), String> {
        self.update_bool("openai_tracing_enabled", enabled)
    }

    pub fn update_use_behavior_trees(&self, enabled: bool) -> Result<(), String> {
        self.update_bool("use_behavior_trees", enabled)
    }

    pub fn update_current_model_id(&self, model_id: Option<i64>) -> Result<(), String> {
        self.ensure_row()?;
        self.conn
            .execute(
                "UPDATE user_settings SET current_model_id = ? WHERE id = 1",
                params![model_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_web_search_enabled(&self, enabled: bool) -> Result<(), String> {
        self.update_bool("web_search_enabled", enabled)
    }

    pub fn update_brave_api_key(&self, api_key: Option<&str>) -> Result<(), String> {
        self.ensure_row()?;
        self.conn
            .execute(
                "UPDATE user_settings SET brave_api_key = ? WHERE id = 1",
                params![api_key],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_max_tool_calls_per_request(&self, value: i32) -> Result<(), String> {
        self.ensure_row()?;
        self.conn
            .execute(
                "UPDATE user_settings SET max_tool_calls_per_request = ? WHERE id = 1",
                params![value],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_oauth_tokens(
        &self,
        access_token: &str,
        refresh_token: &str,
        expires_in: i64,
        account_id: Option<&str>,
    ) -> Result<(), String> {
        self.ensure_row()?;
        let expiry = chrono::Utc::now().timestamp() + expires_in;
        self.conn
            .execute(
                "UPDATE user_settings SET
                 oauth_access_token = ?,
                 oauth_refresh_token = ?,
                 oauth_token_expiry = ?,
                 oauth_account_id = ?,
                 auth_method = 'oauth'
                 WHERE id = 1",
                params![access_token, refresh_token, expiry, account_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear_oauth_tokens(&self) -> Result<(), String> {
        self.ensure_row()?;
        self.conn
            .execute(
                "UPDATE user_settings SET
                 oauth_access_token = NULL,
                 oauth_refresh_token = NULL,
                 oauth_token_expiry = NULL,
                 oauth_account_id = NULL,
                 auth_method = 'api_key'
                 WHERE id = 1",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn update_bool(&self, column: &str, enabled: bool) -> Result<(), String> {
        self.ensure_row()?;
        let sql = format!("UPDATE user_settings SET {} = ? WHERE id = 1", column);
        self.conn
            .execute(&sql, params![if enabled { 1 } else { 0 }])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn ensure_row(&self) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO user_settings (id, openai_tracing_enabled, use_behavior_trees, web_search_enabled, max_tool_calls_per_request) VALUES (1, 0, 0, 0, 50)",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
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
