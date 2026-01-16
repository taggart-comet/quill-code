use super::Error;
use crate::domain::prompting::format_tools_description;
use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolInput, ToolResult};
use std::collections::HashMap;

pub use super::toolsets::GeneralToolset;

/// Trait for toolset implementations that provide a set of tools
///
/// Implementations should create their tools in the `new()` constructor
/// and return a reference to them via the `tools()` method.
///
/// Common methods `get_tool`, `get_tools_description`, and `execute_tool`
/// have default implementations that work with any toolset.
pub trait Toolset {
    /// Returns a reference to the tools map
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>>;

    /// Get a tool by name
    fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools().get(name).map(|t| t.as_ref())
    }

    /// Generate a formatted description of all available tools for LLM planning
    fn get_tools_description(&self) -> String {
        format_tools_description(
            self.tools()
                .iter()
                .map(|(name, tool)| (name.as_str(), tool.as_ref())),
        )
    }

    /// Execute a tool with the given input (XML string)
    fn execute_tool(
        &self,
        tool_name: &str,
        input_xml: &str,
        request: &dyn Request,
    ) -> Result<ToolResult, Error> {
        match self.get_tool(tool_name) {
            Some(tool) => {
                let tool_input = ToolInput::new(input_xml)
                    .map_err(|e| Error::Parse(format!("invalid xml input: {}", e)))?;
                let result = tool.work(&tool_input, request);
                Ok(result)
            }
            None => Err(Error::ToolNotFound(tool_name.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_general_toolset() {
        let toolset = GeneralToolset::new();
        assert!(toolset.get_tool("read_objects").is_some());
        assert!(toolset.get_tool("find_files").is_some());
        assert!(toolset.get_tool("patch_file").is_some());
        assert!(toolset.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_tools_description() {
        let toolset = GeneralToolset::new();
        let description = toolset.get_tools_description();
        assert!(description.contains("Available Tools:"));
        assert!(description.contains("read_objects"));
    }
}
