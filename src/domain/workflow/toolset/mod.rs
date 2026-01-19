mod all;
mod edit;
mod read;
mod none;

pub use all::AllToolset;
pub use edit::EditToolset;
pub use read::ReadToolset;
pub use none::NoneToolset;
#[derive(Clone, Copy)]
pub enum ToolsetType {
    None,
    Read,
    Edit,
    All,
}

impl ToolsetType {
    pub fn build(self) -> Box<dyn Toolset> {
        match self {
            ToolsetType::Read => Box::new(ReadToolset::new()),
            ToolsetType::Edit => Box::new(EditToolset::new()),
            ToolsetType::All => Box::new(AllToolset::new()),
            ToolsetType::None => Box::new(NoneToolset::new()),
        }
    }
}

use super::Error;
use crate::domain::prompting::format_tools_description;
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use std::collections::HashMap;

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

    /// Get tool references for passing into inference engines
    fn tool_refs(&self) -> Vec<&dyn Tool> {
        self.tools().values().map(|tool| tool.as_ref()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_general_toolset() {
        let toolset = AllToolset::new();
        assert!(toolset.get_tool("read_objects").is_some());
        assert!(toolset.get_tool("find_files").is_some());
        assert!(toolset.get_tool("patch_files").is_some());
        assert!(toolset.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_tools_description() {
        let toolset = AllToolset::new();
        let description = toolset.get_tools_description();
        assert!(description.contains("Available Tools:"));
        assert!(description.contains("read_objects"));
    }
}
