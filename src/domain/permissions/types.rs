use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SystemPermissionDecision {
    Ask,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserPermissionDecision {
    AllowOnce,
    AlwaysAllow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionScope {
    Session, // Current session only
    Project, // Current project (persisted)
    Global,  // Across all projects
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: Option<i32>,
    pub tool_name: String,                // "shell", "file_edit", etc.
    pub command_pattern: Option<String>,  // "rm -rf", "git push", etc.
    pub resource_pattern: Option<String>, // "api.search.brave.com", "/src/**", etc.
    pub user_decision: UserPermissionDecision,
    pub scope: PermissionScope,  // Session, Project, Global
    pub project_id: Option<i32>, // If project-scoped
    pub created_at: DateTime<Utc>,
}

#[allow(dead_code)]
impl Permission {
    pub fn new(
        tool_name: String,
        command_pattern: Option<String>,
        resource_pattern: Option<String>,
        decision: UserPermissionDecision,
        scope: PermissionScope,
        project_id: Option<i32>,
    ) -> Self {
        Self {
            id: None,
            tool_name,
            command_pattern,
            resource_pattern,
            user_decision: decision,
            scope,
            project_id,
            created_at: Utc::now(),
        }
    }

    pub fn system_decision(self) -> SystemPermissionDecision {
        if self.user_decision == UserPermissionDecision::AlwaysAllow {
            return SystemPermissionDecision::Allow;
        }
        SystemPermissionDecision::Ask
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub command: Option<String>,
    pub paths: Vec<PathBuf>,
    pub scope: PermissionScope,
    pub project_id: Option<i32>,
    pub is_read_only: bool,
    #[allow(dead_code)]
    pub project_root: PathBuf,
}

impl PermissionRequest {
    pub fn new(
        tool_name: String,
        command: Option<String>,
        paths: Vec<PathBuf>,
        scope: PermissionScope,
        project_id: Option<i32>,
        is_read_only: bool,
        project_root: PathBuf,
    ) -> Self {
        Self {
            tool_name,
            command,
            paths,
            scope,
            project_id,
            is_read_only,
            project_root,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub default_decision: SystemPermissionDecision,
    pub dangerous_commands: Vec<String>,
    pub restricted_paths: Vec<String>,
    pub require_confirmation: bool,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default_decision: SystemPermissionDecision::Ask,
            dangerous_commands: vec![
                "rm -rf".to_string(),
                "sudo".to_string(),
                "chmod 777".to_string(),
                "git push --force".to_string(),
                "dd if=".to_string(),
                "mkfs".to_string(),
            ],
            restricted_paths: vec![
                "/etc".to_string(),
                "/usr/bin".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                ".env".to_string(),
                "~/.ssh".to_string(),
                "~/.aws".to_string(),
                "~/.gnupg".to_string(),
            ],
            require_confirmation: true,
        }
    }
}