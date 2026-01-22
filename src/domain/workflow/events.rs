use crate::domain::tools::FileChange;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangesEvent {
    pub request_id: i64,
    pub changes: Vec<FileChange>,
}
