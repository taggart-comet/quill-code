use super::step::ChainStep;
use super::step::StepType;
use crate::domain::todo::TodoList;
use crate::domain::tools::FileChange;
use crate::domain::tools::ToolResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents an execution chain containing multiple steps
/// The chain is built incrementally as tools are executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub steps: Vec<ChainStep>,
    #[serde(skip)]
    history_steps: Vec<ChainStep>,
    #[serde(skip)]
    pub todo_list: Option<TodoList>,
    pub is_failed: bool,
    pub fail_reason: String,
    #[serde(default)]
    pub final_message: Option<String>,
    pub system_prompt: String,
}

impl Chain {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            history_steps: Vec::new(),
            todo_list: None,
            is_failed: false,
            fail_reason: String::new(),
            final_message: None,
            system_prompt: String::new(),
        }
    }

    /// Add a step to the chain after executing a tool
    pub fn add_step(&mut self, result: ToolResult) {
        if result.tool_name() == "update_todo_list" && result.is_successful() {
            if let Ok(updated_todo_list) = serde_json::from_str::<TodoList>(&result.input_string())
            {
                self.set_todo_list(Some(updated_todo_list));
                return;
            }
        }
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

    /// Add history steps loaded from database
    pub fn add_history(&mut self, history: Vec<ChainStep>) {
        self.history_steps = history;
    }

    /// Set the TODO list for this chain
    pub fn set_todo_list(&mut self, todo_list: Option<TodoList>) {
        self.todo_list = todo_list;
    }

    /// Get current request steps only (for saving to database)
    pub fn get_steps(&self) -> &[ChainStep] {
        &self.steps
    }

    /// Get all steps (history + current) for LLM context
    pub fn get_steps_with_history(&self) -> Vec<ChainStep> {
        let mut all_steps = self.history_steps.clone();
        all_steps.extend(self.steps.clone());
        all_steps
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
        if let Some(last_step) = self.steps.last() {
            if last_step.step_type == StepType::AssistantResponse.as_str() {
                return last_step.tool_output.clone().unwrap_or_default();
            }
        }

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

    pub fn set_system_prompt(&mut self, system_prompt: String) {
        self.system_prompt = system_prompt;
    }

    pub fn merged_file_changes(&self) -> Vec<FileChange> {
        let file_changes: Vec<_> = self
            .steps()
            .iter()
            .filter(|step| step.tool_name.as_deref() == Some("patch_files"))
            .filter_map(|step| step.file_changes.as_ref())
            .flatten()
            .cloned()
            .collect();

        let mut merged_changes: Vec<FileChange> = Vec::new();
        let mut index_by_path: HashMap<String, usize> = HashMap::new();

        for change in file_changes {
            if let Some(&idx) = index_by_path.get(&change.path) {
                let existing = &mut merged_changes[idx];
                existing.added_lines += change.added_lines;
                existing.deleted_lines += change.deleted_lines;
                if !change.unified_diff.is_empty() {
                    if !existing.unified_diff.is_empty() {
                        existing.unified_diff.push('\n');
                    }
                    existing.unified_diff.push_str(&change.unified_diff);
                }
            } else {
                index_by_path.insert(change.path.clone(), merged_changes.len());
                merged_changes.push(change);
            }
        }

        merged_changes
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
