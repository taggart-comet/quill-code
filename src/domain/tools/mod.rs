mod discover_objects;
mod find_files;
mod patch_file;
mod read_objects;
mod shell_exec;
mod structure;

pub use discover_objects::DiscoverObjects;
pub use find_files::FindFiles;
pub use patch_file::PatchFile;
pub use read_objects::ReadObjects;
pub use shell_exec::ShellExec;
pub use structure::Structure;

use crate::domain::session::Request;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid xml: {0}")]
    InvalidXml(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(String),
}

pub struct ToolResult {
    tool_name: String,
    input: String,
    is_successful: bool,
    output: String,
    error_message: String,
}

impl ToolResult {
    pub fn ok(tool_name: String, input: String, output: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: true,
            output,
            error_message: String::new(),
        }
    }

    pub fn error(tool_name: String, input: String, message: String) -> Self {
        Self {
            tool_name,
            input,
            is_successful: false,
            output: String::new(),
            error_message: message.into(),
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
}

/// Helper function to serialize a struct to XML string for tool outputs
///
/// Example:
/// ```rust
/// #[derive(Serialize)]
/// struct MyOutput {
///     result: String,
/// }
///
/// let output = MyOutput { result: "success".to_string() };
/// let xml = serialize_output(&output)?;
/// ToolResult::ok(tool_name, input, xml)
/// ```
pub fn serialize_output<T>(value: &T) -> Result<String, Error>
where
    T: serde::Serialize,
{
    quick_xml::se::to_string(value)
        .map_err(|e| Error::Parse(format!("Failed to serialize to XML: {}", e)))
}

pub fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn parse_input(&self, input: String) -> Option<Error>;
    fn work(&self, request: &dyn Request) -> ToolResult;
    fn parameters(&self) -> Value;
    fn desc(&self) -> String;
}

pub fn build_tool_by_name(name: &str) -> Option<Box<dyn Tool>> {
    match name {
        "discover_objects" => Some(Box::new(DiscoverObjects::new())),
        "read_objects" => Some(Box::new(ReadObjects::new())),
        "find_files" => Some(Box::new(FindFiles::new())),
        "structure" => Some(Box::new(Structure::new())),
        "patch_file" => Some(Box::new(PatchFile::new())),
        "shell_exec" => Some(Box::new(ShellExec::new())),
        _ => None,
    }
}
