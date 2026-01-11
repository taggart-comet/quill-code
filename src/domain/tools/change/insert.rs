use super::Error;
use crate::domain::tools::{Tool, ToolResult};
use crate::utils::insert_content;
use serde::Deserialize;
use serde_yaml::Value as Yaml;

pub struct Insert;

#[derive(Deserialize)]
struct InsertInput {
    full_path_to_file: String,
    target_line: usize,
    #[serde(default)]
    insert_content: String,
}

impl Insert {
    fn parse_input(input: Yaml) -> Result<InsertInput, Error> {
        serde_yaml::from_value(input).map_err(|e| Error::InvalidYaml(e.to_string()))
    }
}

impl Tool for Insert {
    fn name(&self) -> &'static str {
        "change_insert"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        let input_copy = input.clone();

        match Self::parse_input(input) {
            Ok(parsed) => match insert_content(
                &parsed.full_path_to_file,
                parsed.target_line,
                &parsed.insert_content,
            ) {
                Ok(()) => ToolResult::ok(self.name(), input_copy, Yaml::Null),
                Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
            },
            Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
        }
    }

    fn desc(&self) -> &'static str {
        "Insert content at a specific line in an existing file"
    }

    fn input_format(&self) -> &'static str {
        "
input:
  full_path_to_file: string
  target_line: integer
  insert_content: string
"
    }
}
