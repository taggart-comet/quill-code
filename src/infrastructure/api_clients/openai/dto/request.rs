use crate::domain::tools::Tool;
use serde::Serialize;
use serde_json::Value;
use crate::domain::ModelType;

#[derive(Debug, Serialize)]
pub struct RequestDTO {
    model: String,
    instructions: String,
    input: Vec<InputDto>,
    tools: Vec<ToolDto>,
    tool_choice: String,
    parallel_tool_calls: bool,
    store: bool,
    stream: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct InputDto {
    content: Vec<InputContent>,
    role: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct InputContent {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ToolDto {
    r#type: String,
    description: String,
    name: String,
    parameters: Value,
    strict: bool,
}

const ROLE_USER: &str = "user";
const ROLE_SYSTEM: &str = "system";

impl RequestDTO {
    pub(crate) fn new(
        model: String,
        system_prompt: String,
        user_prompt: String,
        tools: &[&dyn Tool],
        chain: &crate::domain::workflow::Chain,
    ) -> Self {
        let mut input = Vec::new();
        input.push(InputDto::from_user_prompt(user_prompt));
        input.extend(InputDto::from_chain(chain));

        Self {
            model,
            instructions: system_prompt,
            input,
            tools: tools.iter().map(|tool| ToolDto::from_tool(*tool)).collect(),
            tool_choice: "auto".to_string(),
            parallel_tool_calls: true,
            store: false,
            stream: false,
        }
    }
}

impl ToolDto {
    pub(super) fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            r#type: "function".to_string(),
            description: tool.desc(),
            name: tool.name().to_string(),
            parameters: tool.parameters(),
            strict: false,
        }
    }
}

impl InputDto {
    fn from_user_prompt(content: String) -> Self {
        Self {
            content: vec![InputContent {
                kind: "input_text".to_string(),
                text: content
            }],
            role: ROLE_USER.to_string(),
            kind: "message".to_string(),
            status: "in_progress".to_string(),
        }
    }

    fn from_chain(chain: &crate::domain::workflow::Chain) -> Vec<Self> {
        chain
            .steps
            .iter()
            .filter(|step| {
                step.step_type == crate::domain::workflow::step::StepType::ToolCall.as_str()
            })
            .filter_map(|step| {
                let tool_name = step.tool_name.as_ref()?;
                let status = if step.is_successful.unwrap_or(false) {
                    "completed"
                } else {
                    "failed"
                };
                Some(Self {
                    content: vec![InputContent {
                        kind: "input_text".to_string(),
                        text: step.get_output(ModelType::OpenAI),
                    }],
                    role: ROLE_SYSTEM.to_string(),
                    kind: "message".to_string(),
                    status: status.to_string(),
                })
            })
            .collect()
    }
}
