use crate::domain::tools::Tool;

/// Format available tools as a description for the LLM
///
/// Generates a formatted list of all available tools with their specs
pub fn format_tools_description<'a>(
    tools: impl Iterator<Item = (&'a str, &'a dyn Tool)>,
) -> String {
    let mut description = String::from("Available Tools:\n\n");

    for (name, tool) in tools {
        description.push_str(&format!("## {}\n{}\n\n", name, tool.spec()));
    }
    description
}
