use thiserror::Error;

#[derive(Debug, Error)]
pub enum AskError {
    #[error("I/O error")]
    IoError,
}
