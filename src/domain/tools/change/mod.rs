mod insert;
mod remove;
mod replace;

pub use insert::Insert;
pub use remove::Remove;
pub use replace::Replace;

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid yaml: {0}")]
    InvalidYaml(String),
}

#[derive(Serialize)]
pub struct ChangeOutput {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ChangeOutput {
    pub fn success() -> String {
        serde_yaml::to_string(&Self { ok: true, error: None }).unwrap_or_else(|_| "ok: true\n".to_string())
    }

    pub fn failure(error: impl Into<String>) -> String {
        serde_yaml::to_string(&Self {
            ok: false,
            error: Some(error.into()),
        })
        .unwrap_or_else(|e| format!("ok: false\nerror: {}\n", e))
    }
}
