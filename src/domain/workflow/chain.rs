use super::{step::ChainStep, Error};
use crate::domain::tools::ToolResult;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as Yaml;

use super::step::StepType;

/// Represents an execution chain containing multiple steps
/// The chain is built incrementally as tools are executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub steps: Vec<ChainStep>,
    pub is_failed: bool,
    pub fail_reason: String,
}

impl Chain {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            is_failed: false,
            fail_reason: String::new(),
        }
    }

    /// Add a step to the chain after executing a tool
    pub fn add_step(&mut self, result: ToolResult) {
        self.steps.push(ChainStep::new(
            StepType::ToolCall,
            Some(result),
        ));
    }

    /// Mark the chain as failed with a reason
    pub fn mark_failed(&mut self, reason: String) {
        self.is_failed = true;
        self.fail_reason = reason;
    }

    /// Add a user interruption step
    pub fn add_interruption(&mut self) {
        self.steps.push(ChainStep::new(
            StepType::UserInterruption,
            None,
        ));
        self.mark_failed("User interrupted".to_string());
    }

    pub fn steps(&self) -> &[ChainStep] {
        &self.steps
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Get the finish message from the finish tool output
    /// Returns the output of the finish tool if found, otherwise returns a default message
    pub fn get_finish_message(&self) -> String {
        // Look for the finish tool step by checking if summary contains "finish"
        if let Some(finish_step) = self.steps.iter().find(|step| 
            step.step_type == StepType::ToolCall.as_str() && 
            step.summary.contains("finish")
        ) {
            // Extract message from summary like "Tool `finish` was executed successfully"
            // For finish tool, we want to return a success message
            if finish_step.summary.contains("successfully") {
                return "The request is fulfilled".to_string();
            }
            finish_step.summary.clone()
        } else {
            // No finish tool found, return a default message
            if self.is_empty() {
                "No steps executed.".to_string()
            } else {
                format!("Workflow completed. Executed {} steps.", self.len())
            }
        }
    }

    /// Get a summary of the chain execution
    /// Returns a string describing how many tool calls were executed and whether it was successful
    /// This should be saved to session_request.result_summary
    pub fn get_summary(&self) -> String {
        let tool_call_count = self.steps.iter()
            .filter(|s| s.step_type == StepType::ToolCall.as_str())
            .count();

        if self.is_failed {
            format!("Executed {} tool calls. Failed: {}", tool_call_count, self.fail_reason)
        } else {
            format!("Executed {} tool calls. Success.", tool_call_count)
        }
    }

    /// Get the log of all steps
    /// Returns text consisting of chain_step.summary for each step
    /// This should be saved to session_request.steps_log
    pub fn get_log(&self) -> String {
        self.steps.iter()
            .map(|step| step.summary.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Parse a single tool choice from LLM output
/// Expected format (possibly wrapped in ```yaml ... ```):
/// ```yaml
/// tool_name: read_objects
/// input:
///   full_path_to_file: cat.py
///   queries:
///     - name: foo
///       kind: method
/// ```
pub fn parse_tool_choice(llm_output: &str) -> Result<(String, Yaml), Error> {
    // Strip markdown code block markers if present
    let content = llm_output
        .trim()
        .strip_prefix("```yaml")
        .or_else(|| llm_output.trim().strip_prefix("```"))
        .unwrap_or(llm_output.trim());

    let content = content
        .trim()
        .strip_suffix("```")
        .unwrap_or(content)
        .trim();

    // Parse the YAML
    let parsed: Yaml = serde_yaml::from_str(content)
        .map_err(|e| Error::Parse(format!("invalid yaml: {}", e)))?;

    // Extract tool_name
    let tool_name = parsed
        .get("tool_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Parse("no tool_name found in LLM output".into()))?;

    if tool_name.is_empty() {
        return Err(Error::Parse("empty tool_name".into()));
    }

    // Extract input field (default to empty mapping if not present)
    let input = parsed
        .get("input")
        .cloned()
        .unwrap_or(Yaml::Mapping(serde_yaml::Mapping::new()));

    Ok((tool_name.to_string(), input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_choice() {
        let output = r#"```yaml
tool_name: read_objects
input:
  full_path_to_file: cat.py
  queries:
    - name: foo
      kind: method
```"#;
        let (tool_name, input) = parse_tool_choice(output).unwrap();
        assert_eq!(tool_name, "read_objects");
        assert_eq!(input["full_path_to_file"].as_str(), Some("cat.py"));
        assert!(input["queries"].is_sequence());
    }

    #[test]
    fn test_parse_tool_choice_no_markers() {
        let output = r#"tool_name: change_replace
input:
  full_path_to_file: main.rs
  start_line: 10
  end_line: 15
  content: |
    fn new_code() {
        println!("hello");
    }"#;
        let (tool_name, input) = parse_tool_choice(output).unwrap();
        assert_eq!(tool_name, "change_replace");
        assert_eq!(input["start_line"].as_u64(), Some(10));
        assert_eq!(input["end_line"].as_u64(), Some(15));
        assert!(input["content"].as_str().is_some());
    }

    #[test]
    fn test_chain_add_step() {
        use crate::domain::tools::ToolResult;
        use crate::domain::workflow::step::StepType;
        use serde_yaml::Value as Yaml;

        let mut chain = Chain::new();
        let result = ToolResult::ok("read_file", Yaml::String("file: test.rs".to_string()), Yaml::String("content here".to_string()));
        chain.add_step(result);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.steps[0].step_type, StepType::ToolCall.as_str());
        assert!(chain.steps[0].summary.contains("read_file"));
        assert!(chain.steps[0].summary.contains("successfully"));
    }

    #[test]
    fn test_chain_get_summary() {
        use crate::domain::tools::ToolResult;
        use serde_yaml::Value as Yaml;

        let mut chain = Chain::new();
        let result = ToolResult::ok("read_file", Yaml::Null, Yaml::Null);
        chain.add_step(result);
        
        let summary = chain.get_summary();
        assert!(summary.contains("Executed 1 tool calls"));
        assert!(summary.contains("Success"));
        assert!(!chain.is_failed);
    }

    #[test]
    fn test_chain_get_log() {
        use crate::domain::tools::ToolResult;
        use serde_yaml::Value as Yaml;

        let mut chain = Chain::new();
        let result1 = ToolResult::ok("read_file", Yaml::Null, Yaml::Null);
        chain.add_step(result1);
        let result2 = ToolResult::ok("find_files", Yaml::Null, Yaml::Null);
        chain.add_step(result2);
        
        let log = chain.get_log();
        assert!(log.contains("read_file"));
        assert!(log.contains("find_files"));
        assert_eq!(log.lines().count(), 2);
    }
}
