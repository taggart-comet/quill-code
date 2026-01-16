use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RequestDTO {
    model: String,
    instructions: String,
    input: Vec<InputDto>,
    tools: Vec<ToolDto>,
    tool_choice: String,
    parallel_tool_calls: bool,
    reasoning: ReasoningConfig,
    store: bool,
    stream: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct InputDto {
    content: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ToolDto {
    r#type: String,
    function: ToolFunction,
}

#[derive(Debug, Serialize)]
pub(super) struct ToolFunction {
    name: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct ReasoningConfig {
    summary: String,
}
