mod discover_objects;
mod find_files;
mod patch_files;
mod read_objects;
mod shell_exec;
mod structure;
mod update_todo_list;
pub mod utils;
mod web_search;

pub use discover_objects::DiscoverObjects;
pub use find_files::FindFiles;
pub use patch_files::PatchFiles;
pub use read_objects::ReadObjects;
pub use shell_exec::ShellExec;
pub use structure::Structure;
pub use update_todo_list::UpdateTodoList;
pub use utils::{short_filename, short_label_from_path, short_words};
pub use web_search::WebSearch;

use crate::domain::session::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    pub unified_diff: String,
}

pub struct ToolResult {
    tool_name: String,
    call_id: String,
    pub(crate) input: String,
    is_successful: bool,
    output: String,
    error_message: String,
    file_changes: Option<Vec<FileChange>>,
}

impl ToolResult {
    pub fn ok(tool_name: String, input: String, output: String, call_id: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: true,
            output,
            error_message: String::new(),
            file_changes: None,
            call_id
        }
    }

    pub fn error(tool_name: String, input: String, message: String, call_id: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: false,
            output: String::new(),
            error_message: message.into(),
            file_changes: None,
            call_id
        }
    }

    pub fn with_file_changes(mut self, changes: Vec<FileChange>) -> Self {
        self.file_changes = Some(changes);
        self
    }

    pub fn apply_output_budget(&mut self, limit: usize) {
        if self.is_successful {
            self.output = utils::truncate_with_notice(&self.output, limit);
        } else {
            self.error_message = utils::truncate_with_notice(&self.error_message, limit);
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

    pub(crate) fn call_id(&self) -> String {
        self.call_id.clone()
    }
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn parse_input(&self, input: String, call_id: String) -> Option<Error>;
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
    fn skip_permission_check(&self) -> bool {
        false
    }
}
