use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionDecision {
    AlwaysAllow,
    AlwaysDeny,
    Ask,       // Prompt user each time
    AllowOnce, // One-time approval
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
    pub decision: PermissionDecision,     // Allow, Deny, Ask
    pub scope: PermissionScope,           // Session, Project, Global
    pub project_id: Option<i32>,          // If project-scoped
    pub created_at: DateTime<Utc>,
}

impl Permission {
    pub fn new(
        tool_name: String,
        command_pattern: Option<String>,
        resource_pattern: Option<String>,
        decision: PermissionDecision,
        scope: PermissionScope,
        project_id: Option<i32>,
    ) -> Self {
        Self {
            id: None,
            tool_name,
            command_pattern,
            resource_pattern,
            decision,
            scope,
            project_id,
            created_at: Utc::now(),
        }
    }

    /// Check if this permission matches the given tool, command, and path
    pub fn matches(&self, tool: &str, command: Option<&str>, path: Option<&PathBuf>) -> bool {
        if self.tool_name != tool {
            return false;
        }

        // Check command pattern if specified
        if let (Some(pattern), Some(cmd)) = (&self.command_pattern, command) {
            if !self.matches_command(pattern, cmd) {
                return false;
            }
        }

        // Check resource pattern if specified
        if let (Some(pattern), Some(p)) = (&self.resource_pattern, path) {
            if !self.matches_path(pattern, p) {
                return false;
            }
        }

        true
    }

    fn matches_command(&self, pattern: &str, command: &str) -> bool {
        // Simple pattern matching - can be enhanced with regex
        if pattern.contains('*') {
            // Basic wildcard matching
            let pattern_parts: Vec<&str> = pattern.split_whitespace().collect();
            let command_parts: Vec<&str> = command.split_whitespace().collect();

            if pattern_parts.len() != command_parts.len() {
                return false;
            }

            pattern_parts
                .iter()
                .zip(command_parts.iter())
                .all(|(p, c)| *p == "*" || *p == *c)
        } else {
            pattern == command
        }
    }

    fn matches_path(&self, pattern: &str, path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy();

        if pattern.contains('*') {
            // Basic glob matching
            if pattern.ends_with("/**") {
                let prefix = pattern.trim_end_matches("/**");
                path_str.starts_with(prefix)
            } else if pattern.starts_with("**/") {
                let suffix = pattern.trim_start_matches("**/");
                path_str.ends_with(suffix)
            } else {
                // Simple wildcard - can be enhanced with proper glob matching
                path_str.contains(pattern.trim_matches('*'))
            }
        } else {
            path_str == pattern
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub command: Option<String>,
    pub paths: Vec<PathBuf>,
    pub scope: PermissionScope,
    pub project_id: Option<i32>,
}

impl PermissionRequest {
    pub fn new(
        tool_name: String,
        command: Option<String>,
        paths: Vec<PathBuf>,
        scope: PermissionScope,
        project_id: Option<i32>,
    ) -> Self {
        Self {
            tool_name,
            command,
            paths,
            scope,
            project_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub default_decision: PermissionDecision,
    pub dangerous_commands: Vec<String>,
    pub restricted_paths: Vec<String>,
    pub require_confirmation: bool,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default_decision: PermissionDecision::Ask,
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
                "~/.ssh".to_string(),
                "~/.aws".to_string(),
                "~/.gnupg".to_string(),
            ],
            require_confirmation: true,
        }
    }
}
