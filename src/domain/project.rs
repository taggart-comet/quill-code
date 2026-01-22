use crate::repository::ProjectRow;
use std::path::{Path, PathBuf};

/// Domain entity representing a project.
/// A project groups related sessions together, typically scoped to a directory.
#[derive(Debug, Clone)]
pub struct Project {
    id: i64,
    name: String,
    project_root: PathBuf,
    _created_at: u64,
    session_count: u64,
}

impl Project {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn session_count(&self) -> u64 {
        self.session_count
    }
}

impl From<ProjectRow> for Project {
    fn from(row: ProjectRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            project_root: row
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            _created_at: row.created_at.parse().unwrap_or(0),
            session_count: row.session_count as u64,
        }
    }
}
