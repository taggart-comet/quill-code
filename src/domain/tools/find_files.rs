use crate::domain::tools::{Tool, ToolResult};
use serde_yaml::Value as Yaml;

pub struct FindFiles;

impl Tool for FindFiles {
    fn name(&self) -> &'static str {
        "find_files"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        ToolResult::ok(
            self.name(),
            input,
            Yaml::String("/Users/maksimtaisov/RustroverProjects/drastis/cats/cat.py".to_string()),
        )
    }

    fn desc(&self) -> &'static str {
        "Find files under a root directory by substring match"
    }

    fn input_format(&self) -> &'static str {
        "
input:
  query: string  # substring to match against path/filename
  root: string   # optional, search root; default is project root
  max_results: integer  # optional, default 20
"
    }
}