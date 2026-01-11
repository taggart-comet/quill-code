pub mod change;
mod find_files;
mod list_objects;
mod read_objects;
mod structure;
mod finish;

pub use find_files::FindFiles;
pub use list_objects::ListObjects;
pub use read_objects::ReadObjects;
pub use structure::Structure;
pub use finish::Finish;

use serde_yaml::Value as Yaml;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid yaml: {0}")]
    InvalidYaml(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(String),
}

pub struct ToolResult {
    tool_name: String,
    input: Yaml,
    is_successful: bool,
    success_output: Yaml,
    error_message: String,
}

impl ToolResult {
    pub fn ok(tool_name: impl Into<String>, input: Yaml, output: Yaml) -> Self {
        Self {
            tool_name: tool_name.into(),
            input,
            is_successful: true,
            success_output: output,
            error_message: "".to_string(),
        }
    }
    pub fn error(tool_name: impl Into<String>, input: Yaml, message: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input,
            is_successful: false,
            success_output: Yaml::Null,
            error_message: message.into(),
        }
    }
    
    pub fn output_string(&self) -> String {
        if self.is_successful {
            serde_yaml::to_string(&self.success_output)
                .unwrap_or_else(|_| format!("{:?}", self.success_output))
        } else {
            format!("Error: {}", self.error_message)
        }
    }

    pub fn input_string(&self) -> String {
        serde_yaml::to_string(&self.input)
            .unwrap_or_else(|_| format!("{:?}", self.input))
    }

    /// Generate a summary string for this tool result
    /// Format: "Tool `tool_name` was executed successfully" or "Tool `tool_name` failed: error_message"
    pub fn summary(&self) -> String {
        if self.is_successful {
            format!("Tool `{}` was executed successfully", self.tool_name)
        } else {
            format!("Tool `{}` failed: {}", self.tool_name, self.error_message)
        }
    }
}

pub trait Tool {
    fn name(&self) -> &'static str;
    fn work(&self, input: Yaml) -> ToolResult;
    fn desc(&self) -> &'static str;
    fn input_format(&self) -> &'static str;
}
