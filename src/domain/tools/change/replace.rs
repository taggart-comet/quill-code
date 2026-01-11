use super::Error;
use crate::domain::tools::{Tool, ToolResult};
use crate::utils::replace_lines;
use serde::Deserialize;
use serde_yaml::Value as Yaml;

pub struct Replace;

#[derive(Deserialize)]
struct ReplaceInput {
    full_path_to_file: String,
    start_line: usize,
    end_line: usize,
    #[serde(default)]
    content: String,
}

impl Replace {
    fn parse_input(input: Yaml) -> Result<ReplaceInput, Error> {
        serde_yaml::from_value(input).map_err(|e| Error::InvalidYaml(e.to_string()))
    }
}

impl Tool for Replace {
    fn name(&self) -> &'static str {
        "change_replace"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        let input_copy = input.clone();
        
        match Self::parse_input(input) {
            Ok(parsed) => {
                match replace_lines(
                    &parsed.full_path_to_file,
                    parsed.start_line,
                    parsed.end_line,
                    &parsed.content,
                ) {
                    Ok(()) => ToolResult::ok(self.name(), input_copy, Yaml::Null),
                    Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
                }
            }
            Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
        }
    }

    fn desc(&self) -> &'static str {
        "Replace content between start_line and end_line (inclusive) with new content"
    }

    fn input_format(&self) -> &'static str {
        "
input:
  full_path_to_file: string
  start_line: integer
  end_line: integer
  content: string
"
    }
}
