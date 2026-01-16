use super::{step::ChainStep, Error};
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
        if let Some(finish_step) = self.steps.iter().find(|step| {
            step.step_type == StepType::ToolCall.as_str() && step.summary.contains("finish")
        }) {
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
            format!("Executed {} tool calls. Success.", tool_call_count)
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
}

/// Parse a single tool choice from LLM output
/// Expected format (possibly wrapped in ```xml ... ```):
/// ```xml
/// <tool_name>read_objects</tool_name>
/// <input>
///   <full_path_to_file>cat.py</full_path_to_file>
///   <queries>
///     <query>
///       <name>foo</name>
///       <kind>method</kind>
///     </query>
///   </queries>
/// </input>
/// ```
pub fn parse_tool_choice(llm_output: &str) -> Result<(String, String), Error> {
    // Strip markdown code block markers if present
    let content = llm_output
        .trim()
        .strip_prefix("```xml")
        .or_else(|| llm_output.trim().strip_prefix("```"))
        .unwrap_or(llm_output.trim());

    let content = content.trim().strip_suffix("```").unwrap_or(content).trim();

    // Parse the XML
    let doc = roxmltree::Document::parse(content)
        .map_err(|e| Error::Parse(format!("invalid xml: {}", e)))?;

    // Extract tool_name
    let tool_name = doc
        .descendants()
        .find(|n| n.has_tag_name("tool_name"))
        .and_then(|n| n.text())
        .ok_or_else(|| Error::Parse("no tool_name found in LLM output".into()))?;

    if tool_name.is_empty() {
        return Err(Error::Parse("empty tool_name".into()));
    }

    // Extract input field (default to empty input if not present)
    let input_xml = doc
        .descendants()
        .find(|n| n.has_tag_name("input"))
        .map(|n| {
            // Serialize the input node back to XML string
            let mut xml = String::new();
            serialize_input_node(&n, &mut xml);
            xml
        })
        .unwrap_or_else(|| "<input></input>".to_string());

    Ok((tool_name.to_string(), input_xml))
}

/// Helper to serialize an input node to XML
fn serialize_input_node(node: &roxmltree::Node, output: &mut String) {
    if node.is_element() {
        output.push('<');
        output.push_str(node.tag_name().name());
        output.push('>');

        for child in node.children() {
            serialize_input_node(&child, output);
        }

        output.push_str("</");
        output.push_str(node.tag_name().name());
        output.push('>');
    } else if node.is_text() {
        if let Some(text) = node.text() {
            output.push_str(&crate::domain::tools::escape_xml(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_choice() {
        let output = r#"```xml
<tool_name>read_objects</tool_name>
<input>
  <full_path_to_file>cat.py</full_path_to_file>
  <queries>
    <query>
      <name>foo</name>
      <kind>method</kind>
    </query>
  </queries>
</input>
```"#;
        let (tool_name, input_xml) = parse_tool_choice(output).unwrap();
        assert_eq!(tool_name, "read_objects");
        assert!(input_xml.contains("cat.py"));
        assert!(input_xml.contains("foo"));
    }

    #[test]
    fn test_parse_tool_choice_no_markers() {
        let output = r#"<tool_name>change_replace</tool_name>
<input>
  <full_path_to_file>main.rs</full_path_to_file>
  <start_line>10</start_line>
  <end_line>15</end_line>
  <content>fn new_code() {
        println!("hello");
    }</content>
</input>"#;
        let (tool_name, input_xml) = parse_tool_choice(output).unwrap();
        assert_eq!(tool_name, "change_replace");
        assert!(input_xml.contains("main.rs"));
        assert!(input_xml.contains("10"));
        assert!(input_xml.contains("15"));
    }

    #[test]
    fn test_chain_add_step() {
        use crate::domain::tools::{ToolInput, ToolResult};
        use crate::domain::workflow::step::StepType;

        let mut chain = Chain::new();
        let input = ToolInput::new("<input><file>test.rs</file></input>").unwrap();
        let result = ToolResult::ok("read_file", &input, "content here");
        chain.add_step(result);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.steps[0].step_type, StepType::ToolCall.as_str());
        assert!(chain.steps[0].summary.contains("read_file"));
        assert!(chain.steps[0].summary.contains("successfully"));
    }

    #[test]
    fn test_chain_get_summary() {
        use crate::domain::tools::{ToolInput, ToolResult};

        let mut chain = Chain::new();
        let input = ToolInput::new("<input></input>").unwrap();
        let result = ToolResult::ok("read_file", &input, "");
        chain.add_step(result);

        let summary = chain.get_summary();
        assert!(summary.contains("Executed 1 tool calls"));
        assert!(summary.contains("Success"));
        assert!(!chain.is_failed);
    }

    #[test]
    fn test_chain_get_log() {
        use crate::domain::tools::{ToolInput, ToolResult};

        let mut chain = Chain::new();
        let input1 = ToolInput::new("<input></input>").unwrap();
        let result1 = ToolResult::ok("read_file", &input1, "");
        chain.add_step(result1);
        let input2 = ToolInput::new("<input></input>").unwrap();
        let result2 = ToolResult::ok("find_files", &input2, "");
        chain.add_step(result2);

        let log = chain.get_log();
        assert!(log.contains("read_file"));
        assert!(log.contains("find_files"));
        assert_eq!(log.lines().count(), 2);
    }
}
