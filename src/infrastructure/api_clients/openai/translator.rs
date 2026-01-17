use super::dto::{RequestDTO, ResponseDTO};
use crate::domain::tools::build_tool_by_name;
use crate::infrastructure::inference::LLMInferenceResult;

pub fn build_request_dto(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    tools: &[&dyn crate::domain::tools::Tool],
    chain: &crate::domain::workflow::Chain,
) -> RequestDTO {
    RequestDTO::new(
        model.to_string(),
        system_prompt.to_string(),
        user_prompt.to_string(),
        tools,
        chain,
    )
}

pub fn build_llm_result(response: ResponseDTO) -> LLMInferenceResult {
    let (summary, tool_call) = response.extract_parts();
    let mut final_summary = summary;
    let chosen_tool = tool_call.and_then(|call| {
        let mut tool = build_tool_by_name(&call.name)?;
        if let Some(err) = tool.parse_input(call.arguments) {
            if final_summary.is_empty() {
                final_summary = format!("Tool input parse error: {}", err);
            }
            return None;
        }
        Some(tool)
    });

    LLMInferenceResult {
        summary: final_summary,
        chosen_tool,
    }
}
