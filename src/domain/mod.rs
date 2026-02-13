mod agent_mode;
mod bt;
pub mod permissions;
mod project;
pub mod prompting;
pub mod session;
pub mod startup;
pub mod todo;
pub mod tools;
mod user_settings;
pub mod workflow;

pub use agent_mode::AgentModeType;
pub use project::Project;
pub use session::service::SessionService;
pub use session::{Session, SessionRequest};
pub use startup::StartupService;
#[allow(unused_imports)]
pub use todo::{TodoItem, TodoList};
pub use user_settings::{AuthMethod, UserSettings};
pub use workflow::{CancellationToken, Chain};

/// Model type enum matching the inference engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    Local,
    OpenAI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAuthType {
    Local,
    ApiKey,
    OAuth,
}

impl ModelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelType::Local => "local",
            ModelType::OpenAI => "openai",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "local" => Some(ModelType::Local),
            "openai" => Some(ModelType::OpenAI),
            _ => None,
        }
    }
}

impl ModelAuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelAuthType::Local => "local",
            ModelAuthType::ApiKey => "api_key",
            ModelAuthType::OAuth => "oauth",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "local" => ModelAuthType::Local,
            "oauth" => ModelAuthType::OAuth,
            _ => ModelAuthType::ApiKey,
        }
    }

    pub fn from_auth_method(method: &AuthMethod) -> Self {
        match method {
            AuthMethod::ApiKey => ModelAuthType::ApiKey,
            AuthMethod::OAuth => ModelAuthType::OAuth,
        }
    }
}
