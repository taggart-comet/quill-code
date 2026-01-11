mod service;

pub use service::{StartupService, StartupConfig};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("repository error: {0}")]
    Repository(String),

    #[error("session not found: {0}")]
    SessionNotFound(i64),
}

impl From<Error> for String {
    fn from(err: Error) -> Self {
        err.to_string()
    }
}