use crate::repository::UserSettingsRow;

#[derive(Debug, Clone)]
pub struct UserSettings {
    _id: i64,
    openai_api_key: Option<String>,
    openai_tracing_enabled: bool,
    _use_behavior_trees: bool,
    _current_model_id: Option<i64>,
    current_model_name: Option<String>,
    web_search_enabled: bool,
    brave_api_key: Option<String>,
}

impl UserSettings {
    pub fn openai_api_key(&self) -> Option<&str> {
        self.openai_api_key.as_deref()
    }

    pub fn openai_tracing_enabled(&self) -> bool {
        self.openai_tracing_enabled
    }

    #[allow(dead_code)]
    pub fn current_model_name(&self) -> Option<&str> {
        self.current_model_name.as_deref()
    }

    pub fn with_current_model_name(mut self, name: Option<String>) -> Self {
        self.current_model_name = name;
        self
    }

    pub fn web_search_enabled(&self) -> bool {
        self.web_search_enabled
    }

    pub fn brave_api_key(&self) -> Option<&str> {
        self.brave_api_key.as_deref()
    }
}

impl From<UserSettingsRow> for UserSettings {
    fn from(row: UserSettingsRow) -> Self {
        Self {
            _id: row.id,
            openai_api_key: row.openai_api_key,
            openai_tracing_enabled: row.openai_tracing_enabled,
            _use_behavior_trees: row.use_behavior_trees,
            _current_model_id: row.current_model_id,
            current_model_name: None,
            web_search_enabled: row.web_search_enabled,
            brave_api_key: row.brave_api_key,
        }
    }
}
