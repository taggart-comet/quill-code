use crate::domain::tools::Tool;
use crate::domain::{Chain, ModelType};
use crate::domain::prompting::format_todo_list_message;
use serde::Serialize;
use serde_json::Value;

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
        _user_prompt: String,
        _images: Vec<String>,
        tools: &[&dyn Tool],
        chain: &crate::domain::workflow::Chain,
    ) -> Self {
        // User request is now part of the chain, no need to add separately
        let input = InputDto::from_chain(chain);

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
    fn from_chain(chain: &Chain) -> Vec<Self> {
        let steps = chain.get_steps_with_history();
        let num_steps = steps.len();

        let mut result: Vec<Self> = steps
            .iter()
            .enumerate()
            .map(|(idx, step)| {
                // Determine status
                let is_last_step = idx == num_steps - 1;
                let is_user_message = step.step_type == "user_message";

                let status = if is_last_step && is_user_message {
                    // Current user message is in progress
                    "in_progress"
                } else if step.is_successful.unwrap_or(false) {
                    "completed"
                } else {
                    "failed"
                };

                // Determine role based on step type
                let role = if step.step_type == "assistant_response" {
                    "assistant".to_string()
                } else if is_user_message {
                    ROLE_USER.to_string()
                } else {
                    ROLE_SYSTEM.to_string() // Tool results remain "system"
                };

                // Build content items
                let content_items = if step.step_type == "assistant_response" {
                    vec![InputContent::output_text(step.get_output(ModelType::OpenAI))]
                } else if is_user_message {
                    // For user messages, include text and images
                    let mut items = vec![InputContent::text(step.input_payload.clone())];

                    // Add image content items if present
                    if let Some(ref images) = step.images {
                        for image_url in images {
                            items.push(InputContent::image(image_url.clone()));
                        }
                    }

                    items
                } else {
                    vec![InputContent::text(step.get_output(ModelType::OpenAI))]
                };

                Self {
                    content: content_items,
                    role,
                    kind: "message".to_string(),
                    status: status.to_string(),
                }
            })
            .collect();

        // Add the plan as system message at the beginning if it exists and is not completed
        if let Some(ref todo_list) = chain.todo_list {
            if !todo_list.is_completed() {
                let todo_content = serde_json::to_string_pretty(&todo_list.items)
                    .unwrap_or_else(|_| "[]".to_string());

                let todo_message = format_todo_list_message(&todo_content);

                let todo_input = Self {
                    content: vec![InputContent::text(todo_message)],
                    role: ROLE_SYSTEM.to_string(),
                    kind: "message".to_string(),
                    status: "in_progress".to_string(),
                };
                result.push(todo_input);
            }
        }

        result
    }
}
