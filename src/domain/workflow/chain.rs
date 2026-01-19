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

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
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

    // Wrap to allow multiple top-level nodes (e.g. <tool_name> + <input>)
    let wrapped = format!("<root>{}</root>", content);

    // Parse the XML
    let doc = roxmltree::Document::parse(&wrapped)
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
        use crate::domain::tools::ToolResult;
        use crate::domain::workflow::step::StepType;

        let mut chain = Chain::new();
        let input = "<input><file>test.rs</file></input>".to_string();
        let result = ToolResult::ok("read_file".to_string(), input, "content here".to_string());
        chain.add_step(result);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.steps[0].step_type, StepType::ToolCall.as_str());
        assert!(chain.steps[0].summary.contains("read_file"));
        assert!(chain.steps[0].summary.contains("successfully"));
    }

    #[test]
    fn test_chain_get_summary() {
        use crate::domain::tools::ToolResult;

        let mut chain = Chain::new();
        let input = "<input></input>".to_string();
        let result = ToolResult::ok("read_file".to_string(), input, "".to_string());
        chain.add_step(result);

        let summary = chain.get_summary();
        assert!(summary.contains("Executed 1 tool calls"));
        assert!(summary.contains("Success"));
        assert!(!chain.is_failed);
    }

    #[test]
    fn test_chain_get_log() {
        use crate::domain::tools::ToolResult;

        let mut chain = Chain::new();
        let input1 = "<input></input>".to_string();
        let result1 = ToolResult::ok("read_file".to_string(), input1, "".to_string());
        chain.add_step(result1);
        let input2 = "<input></input>".to_string();
        let result2 = ToolResult::ok("find_files".to_string(), input2, "".to_string());
        chain.add_step(result2);

        let log = chain.get_log();
        assert!(log.contains("read_file"));
        assert!(log.contains("find_files"));
        assert_eq!(log.lines().count(), 2);
    }
}
