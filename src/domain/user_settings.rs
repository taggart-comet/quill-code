use crate::repository::UserSettingsRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    ApiKey,
    OAuth,
}

#[allow(dead_code)]
impl AuthMethod {
    pub fn as_str(&self) -> &str {
        match self {
            AuthMethod::ApiKey => "api_key",
            AuthMethod::OAuth => "oauth",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "oauth" => AuthMethod::OAuth,
            _ => AuthMethod::ApiKey,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserSettings {
    _id: i64,
    openai_api_key: Option<String>,
    openai_tracing_enabled: bool,
    _use_behavior_trees: bool,
    _current_model_id: Option<i64>,
    current_model_name: Option<String>,
    web_search_enabled: bool,
    brave_api_key: Option<String>,
    max_tool_calls_per_request: i32,
    auth_method: AuthMethod,
    oauth_access_token: Option<String>,
    oauth_refresh_token: Option<String>,
    oauth_token_expiry: Option<i64>,
    oauth_account_id: Option<String>,
}

#[allow(dead_code)]
impl UserSettings {
    pub fn openai_api_key(&self) -> Option<&str> {
        self.openai_api_key.as_deref()
    }

    pub fn openai_tracing_enabled(&self) -> bool {
        self.openai_tracing_enabled
    }

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

    pub fn max_tool_calls_per_request(&self) -> i32 {
        self.max_tool_calls_per_request
    }

    pub fn auth_method(&self) -> &AuthMethod {
        &self.auth_method
    }

    pub fn oauth_access_token(&self) -> Option<&str> {
        self.oauth_access_token.as_deref()
    }

    pub fn oauth_refresh_token(&self) -> Option<&str> {
        self.oauth_refresh_token.as_deref()
    }

    pub fn oauth_account_id(&self) -> Option<&str> {
        self.oauth_account_id.as_deref()
    }

    pub fn is_oauth_token_expired(&self) -> bool {
        if let Some(expiry) = self.oauth_token_expiry {
            return chrono::Utc::now().timestamp() >= expiry;
        }
        false
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
            max_tool_calls_per_request: row.max_tool_calls_per_request,
            auth_method: AuthMethod::from_str(&row.auth_method),
            oauth_access_token: row.oauth_access_token,
            oauth_refresh_token: row.oauth_refresh_token,
            oauth_token_expiry: row.oauth_token_expiry,
            oauth_account_id: row.oauth_account_id,
        }
    }
}
