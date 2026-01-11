use crate::domain::workflow::Chain;
use crate::domain::workflow::Toolset;

/// LLM prompt templates for the coding assistant
///
/// This module contains all prompt templates used for LLM interactions.

/// Create a prompt for the LLM to choose the next tool
pub fn tool_selection_prompt(
    user_prompt: &str,
    toolset: &Toolset,
    chain: &Chain,
) -> String {
    format!(
        "<|im_start|>system\nYou are a coding assistant. Choose ONE next tool to use from the available tools to accomplish the user's request.\n\n{}\n\n<|im_end|>\n\
        <|im_start|>user\n{}---\n{}\n<|im_end|>\n\
        <output_format>\nrespond in a valid yaml format of the chosen tool, specifying input values for the tool, according to it's interface\n<\\output_format>\n\
        <|im_start|>assistant\n",
        toolset.get_tools_description(),
        _format_chain_context(chain),
        user_prompt,
    )
}

fn _format_chain_context(chain: &Chain) -> String {
    use crate::domain::workflow::step::StepType;
    
    let mut context = String::new();
    context.push_str("previous_tool_calls:\n");
    
    // should be in yaml format like:
    // previous_tool_calls:
    // - tool_name: name
    //   execution_order: 1 # autoincremented
    //   status: successful/error
    //   input: Yaml
    //   output: Yaml
    let mut execution_order = 0;
    for step in chain.steps() {
        // Only include tool_call steps, skip interruptions and other step types
        if step.step_type != StepType::ToolCall.as_str() {
            continue;
        }
        
        execution_order += 1;
        
        // Extract tool_name from summary (format: "Tool `name` was executed successfully" or "Tool `name` failed: error")
        let tool_name = if let Some(start) = step.summary.find('`') {
            if let Some(end) = step.summary[start + 1..].find('`') {
                &step.summary[start + 1..start + 1 + end]
            } else {
                "unknown"
            }
        } else {
            "unknown"
        };
        
        // Determine status from summary
        let status = if step.summary.contains("successfully") {
            "successful"
        } else if step.summary.contains("failed") {
            "error"
        } else {
            "unknown"
        };
        
        // Format as YAML
        context.push_str(&format!("- tool_name: {}\n", tool_name));
        context.push_str(&format!("  execution_order: {}\n", execution_order));
        context.push_str(&format!("  status: {}\n", status));
        
        // Format input as YAML (handle multi-line with proper indentation)
        let input_str = step.input_payload.trim();
        if input_str.is_empty() {
            context.push_str("  input: null\n");
        } else if input_str.contains('\n') || input_str.starts_with('{') || input_str.starts_with('[') {
            // Multi-line or complex YAML - use literal block scalar
            context.push_str("  input: |\n");
            for line in input_str.lines() {
                context.push_str(&format!("    {}\n", line));
            }
        } else {
            // Single line - can be inline
            context.push_str(&format!("  input: {}\n", input_str));
        }
        
        // Format output as YAML (handle multi-line with proper indentation)
        let output_str = step.context_payload.trim();
        if output_str.is_empty() {
            context.push_str("  output: null\n");
        } else if output_str.contains('\n') || output_str.starts_with('{') || output_str.starts_with('[') {
            // Multi-line or complex YAML - use literal block scalar
            context.push_str("  output: |\n");
            for line in output_str.lines() {
                context.push_str(&format!("    {}\n", line));
            }
        } else {
            // Single line - can be inline
            context.push_str(&format!("  output: {}\n", output_str));
        }
    }
    
    context
}


