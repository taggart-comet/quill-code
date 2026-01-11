use super::Error;
use crate::domain::prompting::format_tools_description;
use crate::domain::tools::{Tool, ToolResult, FindFiles, ListObjects, ReadObjects, Structure};
use serde_yaml::Value as Yaml;
use std::collections::HashMap;

/// Manages the available tools and provides descriptions for LLM planning
pub struct Toolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl Toolset {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();
        
        // Register all available tools
        let list_objects = Box::new(ListObjects);
        tools.insert(list_objects.name().to_string(), list_objects);

        let read_objects = Box::new(ReadObjects);
        tools.insert(read_objects.name().to_string(), read_objects);
        
        let find_files = Box::new(FindFiles);
        tools.insert(find_files.name().to_string(), find_files);
        
        let structure = Box::new(Structure);
        tools.insert(structure.name().to_string(), structure);

        let finish = Box::new(crate::domain::tools::Finish);
        tools.insert(finish.name().to_string(), finish);

        let insert = Box::new(crate::domain::tools::change::Insert);
        tools.insert(insert.name().to_string(), insert);

        let remove = Box::new(crate::domain::tools::change::Remove);
        tools.insert(remove.name().to_string(), remove);

        let replace = Box::new(crate::domain::tools::change::Replace);
        tools.insert(replace.name().to_string(), replace);

        Self { tools }
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Generate a formatted description of all available tools for LLM planning
    pub fn get_tools_description(&self) -> String {
        format_tools_description(self.tools.iter().map(|(name, tool)| (name.as_str(), tool.as_ref())))
    }

    /// Execute a tool with the given input
    pub fn execute_tool(&self, tool_name: &str, input: Yaml) -> Result<ToolResult, Error> {
        match self.get_tool(tool_name) {
            Some(tool) => {
                let result = tool.work(input);
                Ok(result)
            }
            None => Err(Error::ToolNotFound(tool_name.into()))
        }
    }
}

impl Default for Toolset {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tool() {
        let toolset = Toolset::new();
        assert!(toolset.get_tool("read_objects").is_some());
        assert!(toolset.get_tool("find_files").is_some());
        assert!(toolset.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_tools_description() {
        let toolset = Toolset::new();
        let description = toolset.get_tools_description();
        assert!(description.contains("Available Tools:"));
        assert!(description.contains("read_objects"));
        assert!(description.contains("finish"));
    }
}
