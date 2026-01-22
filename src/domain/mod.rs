mod project;
mod user_settings;
pub mod prompting;
pub mod session;
pub mod startup;
pub mod tools;
pub mod workflow;
pub mod permissions;
mod bt;

pub use project::Project;
pub use session::service::SessionService;
pub use session::{Session, SessionRequest};
pub use startup::StartupService;
pub use user_settings::UserSettings;
pub use workflow::{CancellationToken, Chain};

/// Model type enum matching the inference engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    Local,
    OpenAI,
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
