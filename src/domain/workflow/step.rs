use crate::domain::tools::{FileChange, ToolResult};
use crate::domain::{prompting, ModelType};
use serde::{Deserialize, Serialize};

/// Types of steps that can occur in an execution chain
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepType {
    /// A tool was called and executed
    ToolCall,
    /// User interrupted the workflow (Ctrl+C)
    UserInterruption,
    /// Signifies that a step in the behavior tree was completed
    BehaviorTreeStepPassed,
    /// Assistant's text response (reasoning, questions, explanations)
    AssistantResponse,
    /// User's message/input
    UserMessage,
}

impl StepType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepType::ToolCall => "tool_call",
            StepType::UserInterruption => "user_interruption",
            StepType::BehaviorTreeStepPassed => "behavior_tree_step_passed",
            StepType::AssistantResponse => "assistant_response",
            StepType::UserMessage => "user_message",
        }
    }
}

/// Represents a single step in an execution chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    pub step_type: String,
    pub summary: String,
    pub context_payload: String,
    pub input_payload: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_output: Option<String>,
    #[serde(default)]
    pub is_successful: Option<bool>,
    #[serde(default)]
    pub file_changes: Option<Vec<FileChange>>,
    #[serde(default)]
    pub images: Option<Vec<String>>,
}

impl ChainStep {
    pub fn new(step_type: StepType, tool_result: Option<ToolResult>) -> Self {
        let mut summary = String::new();
        if step_type == StepType::UserInterruption {
            summary = "User interrupted".to_string();
        }
        let mut context_payload = String::new();
        let mut input_payload = String::new();
        let mut tool_name = None;
        let mut tool_output = None;
        let mut is_successful = None;
        let mut file_changes = None;
        if let Some(tr) = tool_result {
            summary = tr.summary();
            context_payload = tr.output_string();
            input_payload = tr.input_string();
            tool_name = Some(tr.tool_name().to_string());
            tool_output = Some(tr.output_raw().to_string());
            is_successful = Some(tr.is_successful());
            file_changes = tr.file_changes().map(|fc| fc.to_vec());
        }

        Self {
            step_type: step_type.as_str().to_string(),
            summary,
            context_payload,
            input_payload,
            tool_name,
            tool_output,
            is_successful,
            file_changes,
            images: None,
        }
    }

    /// Create a step for an assistant's text response
    pub fn assistant_response(summary: String, raw_output: String) -> Self {
        Self {
            step_type: StepType::AssistantResponse.as_str().to_string(),
            summary,
            context_payload: raw_output.clone(),
            input_payload: String::new(),
            tool_name: None,
            tool_output: Some(raw_output),
            is_successful: Some(true),
            file_changes: None,
            images: None,
        }
    }

    /// Create a step for a user's message
    pub fn user_message(prompt: String, images: Vec<String>) -> Self {
        let summary = if images.is_empty() {
            prompt.clone()
        } else {
            format!("{} (with {} image(s))", prompt, images.len())
        };

        let images_opt = if images.is_empty() {
            None
        } else {
            Some(images)
        };

        Self {
            step_type: StepType::UserMessage.as_str().to_string(),
            summary,
            context_payload: prompt.clone(),
            input_payload: prompt,
            tool_name: None,
            tool_output: None,
            is_successful: Some(true),
            file_changes: None,
            images: images_opt,
        }
    }

    pub fn get_output(&self, model_type: ModelType) -> String {
        if self.step_type == StepType::ToolCall.as_str() {
            return prompting::get_tool_result(model_type, self.clone());
        }

        // Return assistant responses as-is
        if self.step_type == StepType::AssistantResponse.as_str() {
            return self.tool_output.clone().unwrap_or_default();
        }

        let mut output = format!("Previous step `{}`: {}", self.step_type, self.input_payload);
        output.push_str(&format!("\nStep output: {}", self.summary));
        output
    }
}
