use crate::domain::tools::Tool;
use crate::domain::{Chain, ModelType};
use crate::domain::prompting::format_todo_list_message;
use serde::Serialize;
use serde_json::Value;
use crate::domain::workflow::step::StepType;

#[derive(Debug, Serialize)]
pub struct RequestDTO {
    model: String,
    instructions: String,
    input: Vec<InputMessageDto>,
    tools: Vec<ToolDto>,
    tool_choice: String,
    parallel_tool_calls: bool,
    store: bool,
    stream: bool,
}

#[derive(Debug, Serialize)]
pub(in crate::infrastructure) struct MessageDto {
    content: Vec<InputContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Serialize)]
pub(in crate::infrastructure) struct FunctionOutputDto {
    output: String,
    #[serde(rename = "type")]
    kind: String,
    call_id: String,
}

#[derive(Debug, Serialize)]
pub(in crate::infrastructure) struct FunctionCallDto {
    arguments: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    call_id: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum InputMessageDto {
    Message(MessageDto),
    FunctionOutput(FunctionOutputDto),
    FunctionCall(FunctionCallDto),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum InputContent {
    Text {
        #[serde(rename = "type")]
        kind: String, // "input_text" or "output_text"
        text: String,
    },
    Image {
        #[serde(rename = "type")]
        kind: String, // "input_image"
        image_url: String, // data:image/png;base64,...
    },
}

impl InputContent {
    pub fn text(text: String) -> Self {
        Self::Text {
            kind: "input_text".to_string(),
            text,
        }
    }

    pub fn output_text(text: String) -> Self {
        Self::Text {
            kind: "output_text".to_string(),
            text,
        }
    }

    pub fn image(data_url: String) -> Self {
        Self::Image {
            kind: "input_image".to_string(),
            image_url: data_url,
        }
    }

}

impl FunctionOutputDto {
    pub fn new(output: String, call_id: String) -> Self {
        Self {
            kind: "function_call_output".to_string(),
            output,
            call_id,
        }
    }
}

impl FunctionCallDto {
    pub fn new(name: String, arguments: String, call_id: String) -> Self {
        Self {
            name,
            kind: "function_call".to_string(),
            arguments,
            call_id,
        }
    }
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
const ROLE_ASSISTANT: &str = "assistant";

impl RequestDTO {
    pub(crate) fn new(
        model: String,
        tools: &[&dyn Tool],
        chain: &Chain,
    ) -> Self {
        // User request is now part of the chain, no need to add separately
        let input = MessageDto::build(chain);

        Self {
            model,
            instructions: chain.system_prompt.clone(),
            input,
            tools: tools.iter().map(|tool| ToolDto::from_tool(*tool)).collect(),
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
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

impl MessageDto {
    fn build(chain: &Chain) -> Vec<InputMessageDto> {
        let steps = chain.get_steps_with_history();

        let mut result: Vec<InputMessageDto> = Vec::new();

        for step in steps.iter() {
            let is_user_message = step.step_type == StepType::UserMessage.as_str();

                if is_user_message {
                    // User message: text + optional images
                    let mut items = vec![InputContent::text(step.input_payload.clone())];

                    if let Some(ref images) = step.images {
                        for image_url in images {
                            items.push(InputContent::image(image_url.clone()));
                        }
                    }

                    result.push(InputMessageDto::Message(Self {
                        content: items,
                        role: Some(ROLE_USER.to_string()),
                        kind: "message".to_string(),
                    }));
                } else if step.step_type == StepType::ToolCall.as_str() {

                    let tool_name = step.tool_name.clone().unwrap();
                    let call_id = step.call_id.clone().unwrap();
                    // Tool call output is a separate DTO type
                result.push(InputMessageDto::FunctionCall(FunctionCallDto::new(tool_name, step.input_payload.clone(), call_id.clone())));
                result.push(InputMessageDto::FunctionOutput(FunctionOutputDto::new(step.get_output(ModelType::OpenAI), call_id)));
                } else {
                    // Assistant message
                    result.push(InputMessageDto::Message(Self {
                        content: vec![InputContent::output_text(step.get_output(ModelType::OpenAI))],
                        role: Some(ROLE_ASSISTANT.to_string()),
                        kind: "message".to_string(),
                    }));
                }
        }

        // Add the plan as system message at the beginning if it exists and is not completed
        if let Some(ref todo_list) = chain.todo_list {
            if !todo_list.is_completed() {
                let todo_content = serde_json::to_string_pretty(&todo_list.items)
                    .unwrap_or_else(|_| "[]".to_string());

                let todo_message = format_todo_list_message(&todo_content);

                let todo_input = Self {
                    content: vec![InputContent::text(todo_message)],
                    role: Some(ROLE_SYSTEM.to_string()),
                    kind: "message".to_string(),
                };

                // Put it first
                result.insert(0, InputMessageDto::Message(todo_input));
            }
        }

        result
    }
}