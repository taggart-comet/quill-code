use crate::domain::prompting;
use crate::domain::session::Request;
use crate::domain::workflow::Chain;
use crate::domain::workflow::Toolset;
use crate::domain::ModelType;

/// LLM prompt templates for the coding assistant
///
/// This module contains all prompt templates used for LLM interactions.

/// Create a prompt for the LLM to choose the next tool
pub fn main_request_prompt(
    model_type: ModelType,
    request: &dyn Request,
    toolset: &dyn Toolset,
    chain: &Chain,
) -> String {
    let user_prompt = prompting::format_session_prompt(model_type, request);

    if model_type == ModelType::OpenAI {
        format!(
            "You are a coding assistant that chooses the NEXT tool to run.\n\
\n\
RULES:\n\
- Choose exactly ONE tool from AVAILABLE_TOOLS.\n\
- Respond with ONLY valid XML. No markdown fences, no explanations, no extra text.\n\
- XML must contain exactly: <tool_name>name</tool_name> and <input>...</input>.\n\
- Use the tool's declared input fields only.\n\
- If required info is missing, choose the most appropriate discovery tool (e.g., find_files / structure / list_objects / read_objects).\n\
\n\
AVAILABLE_TOOLS:\n\
{}\n\
\n\
CONTEXT:\n\
{}\n\
\n\
USER_REQUEST:\n\
{}\n\
\n\
OUTPUT (XML ONLY):\n",
            toolset.get_tools_description(),
            _format_chain_context(chain),
            user_prompt,
        )
    } else {
        format!(
            "<|im_start|>system\nYou are a coding assistant. Choose ONE next tool to use from the available tools to accomplish the user's request.\n\n{}\n\n<|im_end|>\n\
            <|im_start|>user\n{}\n{}\n<|im_end|>\n\
            <output_format>\nrespond in a valid xml format of the chosen tool, specifying input values for the tool, according to it's interface\n<\\output_format>\n\
            <|im_start|>assistant\n",
            toolset.get_tools_description(),
            user_prompt,
            _format_chain_context(chain),
        )
    }
}

fn _format_chain_context(chain: &Chain) -> String {
    use crate::domain::workflow::step::StepType;

    let mut context = String::new();
    context.push_str("<previous_tool_calls>\n");

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

        // Format as XML
        context.push_str("  <tool_call>\n");
        context.push_str(&format!("    <tool_name>{}</tool_name>\n", tool_name));
        context.push_str(&format!(
            "    <execution_order>{}</execution_order>\n",
            execution_order
        ));
        context.push_str(&format!("    <status>{}</status>\n", status));

        // Format input as XML
        let input_str = step.input_payload.trim();
        if input_str.is_empty() {
            context.push_str("    <input></input>\n");
        } else {
            context.push_str("    <input>");
            // Escape XML in input
            let escaped = input_str
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            context.push_str(&escaped);
            context.push_str("</input>\n");
        }

        // Format output as XML
        let output_str = step.context_payload.trim();
        if output_str.is_empty() {
            context.push_str("    <output></output>\n");
        } else {
            context.push_str("    <output>");
            // Escape XML in output
            let escaped = output_str
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            context.push_str(&escaped);
            context.push_str("</output>\n");
        }
        context.push_str("  </tool_call>\n");
    }

    context.push_str("</previous_tool_calls>\n");
    context
}
