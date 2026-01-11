use crate::domain::tools::{Tool, ToolResult};
use serde_yaml::Value as Yaml;

pub struct Structure;

impl Tool for Structure {
    fn name(&self) -> &'static str {
        "structure"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        ToolResult::ok(self.name(), input, Yaml::String(String::new()))
    }

    fn desc(&self) -> &'static str {
        "Return the directory structure up to a given depth"
    }

    fn input_format(&self) -> &'static str {
        "
input:
  path: string
  max_depth: integer
"
    }
}
