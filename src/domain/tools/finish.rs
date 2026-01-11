use crate::domain::tools::{Tool, ToolResult};
use serde_yaml::Value as Yaml;

pub struct Finish;

impl Tool for Finish {
    fn name(&self) -> &'static str {
        "finish"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        ToolResult::ok(
            self.name(),
            input,
            Yaml::String("The request is fulfilled".to_string()),
        )
    }

    fn desc(&self) -> &'static str {
        "Finish working on user request, as everything asked for has been done."
    }

    fn input_format(&self) -> &'static str {
        ""
    }
}
