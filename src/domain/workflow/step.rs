use crate::domain::tools::ToolResult;
use serde::{Deserialize, Serialize};
use crate::domain::{prompting, ModelType};

/// Types of steps that can occur in an execution chain
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepType {
    /// A tool was called and executed
    ToolCall,
    /// User interrupted the workflow (Ctrl+C)
    UserInterruption,
    /// A todo item was created
    TodoCreation,
    /// A todo item was updated
    TodoUpdate,
}

impl StepType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepType::ToolCall => "tool_call",
            StepType::UserInterruption => "user_interruption",
            StepType::TodoCreation => "todo_creation",
            StepType::TodoUpdate => "todo_update",
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
        if let Some(tr) = tool_result {
            summary = tr.summary();
            context_payload = tr.output_string();
            input_payload = tr.input_string();
            tool_name = Some(tr.tool_name().to_string());
            tool_output = Some(tr.output_raw().to_string());
            is_successful = Some(tr.is_successful());
        }

        Self {
            step_type: step_type.as_str().to_string(),
            summary,
            context_payload,
            input_payload,
            tool_name,
            tool_output,
            is_successful,
        }
    }

    pub fn get_output(&self, model_type: ModelType) -> String {
        prompting::get_tool_result(model_type, self.clone())
    }
}
