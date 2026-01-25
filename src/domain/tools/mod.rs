mod discover_objects;
mod find_files;
mod patch_files;
mod read_objects;
mod shell_exec;
mod structure;
mod web_search;

pub use discover_objects::DiscoverObjects;
pub use find_files::FindFiles;
pub use patch_files::PatchFiles;
pub use read_objects::ReadObjects;
pub use shell_exec::ShellExec;
pub use structure::Structure;
pub use web_search::WebSearch;

use crate::domain::session::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(String),
}

pub const TOOL_OUTPUT_BUDGET_CHARS: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub added_lines: u32,
    pub deleted_lines: u32,
}

pub struct ToolResult {
    tool_name: String,
    input: String,
    is_successful: bool,
    output: String,
    error_message: String,
    file_changes: Option<Vec<FileChange>>,
}

impl ToolResult {
    pub fn ok(tool_name: String, input: String, output: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: true,
            output,
            error_message: String::new(),
            file_changes: None,
        }
    }

    pub fn error(tool_name: String, input: String, message: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: false,
            output: String::new(),
            error_message: message.into(),
            file_changes: None,
        }
    }

    pub fn with_file_changes(mut self, changes: Vec<FileChange>) -> Self {
        self.file_changes = Some(changes);
        self
    }

    pub fn apply_output_budget(&mut self, limit: usize) {
        if self.is_successful {
            self.output = truncate_with_notice(&self.output, limit);
        } else {
            self.error_message = truncate_with_notice(&self.error_message, limit);
        }
    }

    pub fn output_string(&self) -> String {
        if self.is_successful {
            self.output.clone()
        } else {
            format!("Error: {}", self.error_message)
        }
    }

    pub fn input_string(&self) -> String {
        self.input.clone()
    }

    pub fn is_successful(&self) -> bool {
        self.is_successful
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn output_raw(&self) -> &str {
        if self.is_successful {
            &self.output
        } else {
            &self.error_message
        }
    }

    /// Generate a summary string for this tool result
    pub fn summary(&self) -> String {
        if self.is_successful {
            format!("Tool `{}` was executed successfully", self.tool_name)
        } else {
            format!("Tool `{}` failed: {}", self.tool_name, self.error_message)
        }
    }

    pub fn file_changes(&self) -> Option<&[FileChange]> {
        self.file_changes.as_deref()
    }
}

fn truncate_with_notice(text: &str, limit: usize) -> String {
    let current_len = text.chars().count();
    if current_len <= limit {
        return text.to_string();
    }

    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push_str(&format!(
        "\n[output truncated to {} chars; refine your query to limit output]",
        limit
    ));
    truncated
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn parse_input(&self, input: String) -> Option<Error>;
    fn work(&self, request: &dyn Request) -> ToolResult;
    fn parameters(&self) -> Value;
    fn desc(&self) -> String;
    fn get_input(&self) -> String;
    fn get_progress_message(&self, _request: &dyn Request) -> String {
        format!("Running {}", self.name())
    }
    fn get_output_budget(&self) -> Option<usize> {
        None
    }

    // Permission-related methods
    fn get_command(&self, _request: &dyn Request) -> Option<String> {
        None
    }
    fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
        vec![]
    }
    fn is_read_only(&self) -> bool {
        false
    }
}

pub fn short_filename(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "files".to_string();
    }
    let name = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);
    if name.is_empty() {
        "files".to_string()
    } else {
        name.to_string()
    }
}

pub fn short_label_from_path(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.contains('/') || trimmed.contains('\\') {
        short_filename(trimmed)
    } else {
        trimmed.to_string()
    }
}

pub fn short_words(input: &str, max_words: usize) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut words = trimmed.split_whitespace();
    let mut parts = Vec::new();
    for _ in 0..max_words {
        if let Some(word) = words.next() {
            parts.push(word);
        } else {
            break;
        }
    }
    let has_more = words.next().is_some();
    let base = parts.join(" ");
    if base.is_empty() {
        String::new()
    } else if has_more {
        format!("{}...", base)
    } else {
        base
    }
}

pub fn build_tool_by_name(name: &str) -> Option<Box<dyn Tool>> {
    match name {
        "discover_objects" => Some(Box::new(DiscoverObjects::new())),
        "read_objects" => Some(Box::new(ReadObjects::new())),
        "find_files" => Some(Box::new(FindFiles::new())),
        "structure" => Some(Box::new(Structure::new())),
        "patch_files" => Some(Box::new(PatchFiles::new())),
        "shell_exec" => Some(Box::new(ShellExec::new())),
        "web_search" => Some(Box::new(WebSearch::new())),
        _ => None,
    }
}
