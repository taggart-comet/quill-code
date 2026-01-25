use super::step::ChainStep;
use crate::domain::tools::ToolResult;
use serde::{Deserialize, Serialize};

use super::step::StepType;

/// Represents an execution chain containing multiple steps
/// The chain is built incrementally as tools are executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub steps: Vec<ChainStep>,
    pub is_failed: bool,
    pub fail_reason: String,
    #[serde(default)]
    pub final_message: Option<String>,
}

impl Chain {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            is_failed: false,
            fail_reason: String::new(),
            final_message: None,
        }
    }

    /// Add a step to the chain after executing a tool
    pub fn add_step(&mut self, result: ToolResult) {
        self.steps
            .push(ChainStep::new(StepType::ToolCall, Some(result)));
    }

    /// Mark the chain as failed with a reason
    pub fn mark_failed(&mut self, reason: String) {
        self.is_failed = true;
        self.fail_reason = reason;
    }

    /// Add a user interruption step
    pub fn add_interruption(&mut self) {
        self.steps
            .push(ChainStep::new(StepType::UserInterruption, None));
        self.mark_failed("User interrupted".to_string());
    }

    pub fn steps(&self) -> &[ChainStep] {
        &self.steps
    }

    pub fn set_final_message(&mut self, message: String) {
        self.final_message = Some(message);
    }

    pub fn final_message(&self) -> Option<&str> {
        self.final_message.as_deref()
    }

    /// Get a summary of the chain execution
    /// Returns a string describing how many tool calls were executed and whether it was successful
    /// This should be saved to session_request.result_summary
    pub fn get_summary(&self) -> String {
        let tool_call_count = self
            .steps
            .iter()
            .filter(|s| s.step_type == StepType::ToolCall.as_str())
            .count();

        if self.is_failed {
            format!(
                "Executed {} tool calls. Failed: {}",
                tool_call_count, self.fail_reason
            )
        } else {
            format!("Success. Executed {} tool calls. ", tool_call_count)
        }
    }

    /// Get the log of all steps
    /// Returns text consisting of chain_step.summary for each step
    /// This should be saved to session_request.steps_log
    pub fn get_log(&self) -> String {
        self.steps
            .iter()
            .map(|step| step.summary.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[allow(dead_code)]
    pub fn total_payload_len_chars(&self) -> usize {
        let mut total = 0usize;

        for step in &self.steps {
            total += step.summary.chars().count();
            total += step.context_payload.chars().count();
            total += step.input_payload.chars().count();
            if let Some(output) = &step.tool_output {
                total += output.chars().count();
            }
        }

        if let Some(message) = &self.final_message {
            total += message.chars().count();
        }

        total
    }
}