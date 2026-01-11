use crate::domain::Chain;
use crate::domain::tools::Tool;
use crate::domain::workflow::Toolset;

/// Format available tools as a description for the LLM
///
/// Generates a formatted list of all available tools with their descriptions
/// and input formats
pub fn format_tools_description<'a>(tools: impl Iterator<Item = (&'a str, &'a dyn Tool)>) -> String {
    let mut description = String::from("Available Tools:\n\n");

    for (name, tool) in tools {
        description.push_str(&format!("Description: {}\n", tool.desc()));
        description.push_str("Format:\n");
        description.push_str(&format!("```yaml\ntool_name: {}{}```\n\n", name, tool.input_format()));
    }
    description
}

