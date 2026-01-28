use super::dto::{RequestDTO, ResponseDTO};
use crate::infrastructure::inference::{LLMInferenceResult, ToolCall};

pub fn build_request_dto(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    images: &[String],
    tools: &[&dyn crate::domain::tools::Tool],
    chain: &crate::domain::workflow::Chain,
    mut tracer: Option<&mut openai_agents_tracing::TracingFacade>,
) -> RequestDTO {
    if let Some(ref mut tracer) = tracer {
        if !images.is_empty() {
            tracer.start_span(
                "request_build_with_images",
                openai_agents_tracing::SpanKind::Function,
            );
            tracer.add_input(
                "request_build_with_images",
                format!("Building request with {} images", images.len()),
            );
        }
    }

    let dto = RequestDTO::new(
        model.to_string(),
        system_prompt.to_string(),
        user_prompt.to_string(),
        tools,
        chain,
    );

    if let Some(ref mut tracer) = tracer {
        if !images.is_empty() {
            tracer.add_output(
                "request_build_with_images",
                "Request DTO created with multimodal content".to_string(),
            );
            tracer.end_span("request_build_with_images");
        }
    }

    dto
}

pub fn build_llm_result(
    response: ResponseDTO,
    _tools: &[&dyn crate::domain::tools::Tool],
) -> LLMInferenceResult {
    let (summary, tool_call_dto) = response.extract_parts();
    let raw_output = summary.clone();
    let final_summary = summary;

    let tool_call = if let Some(call) = tool_call_dto {
        // Create ToolCall for the workflow to use
        Some(ToolCall {
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        })
    } else {
        None
    };

    LLMInferenceResult {
        summary: final_summary,
        raw_output,
        tool_call,
    }
}
