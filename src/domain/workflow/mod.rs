pub mod chain;
pub mod step;
pub mod tool_runner;
pub mod toolset;
pub mod workflow;

pub use chain::Chain;
pub use step::ChainStep;
#[allow(unused_imports)]
pub use tool_runner::ToolRunner;
pub use toolset::AllToolset;
pub use workflow::Workflow;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// A token that can be used to signal cancellation to running operations.
/// Clone is cheap - it just clones the Arc.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if cancellation has been requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Reset the token for reuse
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("no prompt set in session")]
    NoPromptSet,

    #[error("inference failed: {0}")]
    Inference(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("cancelled")]
    Cancelled,
}

impl From<Error> for String {
    fn from(err: Error) -> Self {
        err.to_string()
    }
}