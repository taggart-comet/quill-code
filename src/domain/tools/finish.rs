use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolInput, ToolResult};

pub struct Finish;

impl Tool for Finish {
    fn name(&self) -> &'static str {
        "finish"
    }

    fn work(&self, input: &ToolInput, _request: &dyn Request) -> ToolResult {
        ToolResult::ok(self.name(), input, "The request is fulfilled".to_string())
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{}` tool when the task was accomplished or you have a question to the user. Fill the input format precisely:

<tool_name>{}</tool_name>
<input>
  <message_for_user>describe here, what was done</message_for_user>
</input>
"#,
            self.name(),
            self.name()
        )
    }
}
