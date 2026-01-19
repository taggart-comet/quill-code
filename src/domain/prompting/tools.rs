use crate::domain::tools::Tool;

/// Format available tools as a description for the LLM
///
/// Generates a formatted list of all available tools with their descriptions
pub fn format_tools_description<'a>(
    tools: impl Iterator<Item = (&'a str, &'a dyn Tool)>,
) -> String {
    let mut description = String::from("Available Tools:\n\n");

    for (name, tool) in tools {
        description.push_str(&format!("## {}\n{}\n\n", name, tool.desc()));
    }
    description
}

pub fn get_tool_result(
    _model_type: crate::domain::ModelType,
    chain_step: crate::domain::workflow::ChainStep,
) -> String {
    let tool_name = chain_step.tool_name.unwrap_or_else(|| "unspecified".to_string());
    let output = chain_step.tool_output.unwrap_or_default();
    format!(
        "Tool `{}` execution output is: \n{}\n\
---\n\
Tool input was: \n\
{}\n",
        tool_name, output, chain_step.input_payload
    )
}
