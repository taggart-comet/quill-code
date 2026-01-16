use super::{Error, Tool, ToolInput, ToolResult};
use crate::domain::session::Request;
use crate::utils::{Lang, ObjectKind, ParsedObject, UniversalParser};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

pub struct ReadObjects;

#[derive(Debug, Serialize)]
pub struct ObjectContent {
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ObjectQuery {
    pub kind: Option<ObjectKind>,
    pub name: String,
}

impl ObjectQuery {
    pub fn new(name: &str) -> Self {
        Self {
            kind: None,
            name: name.to_string(),
        }
    }

    pub fn with_kind(kind: ObjectKind, name: &str) -> Self {
        Self {
            kind: Some(kind),
            name: name.to_string(),
        }
    }

    fn matches(&self, obj: &ParsedObject) -> bool {
        let name_matches = obj.name == self.name || obj.name.contains(&self.name);

        match self.kind {
            Some(kind) => kind == obj.kind && name_matches,
            None => name_matches,
        }
    }
}

#[derive(Serialize)]
struct ReadObjectsOutput {
    language: String,
    object_not_found: bool,
    results: HashMap<String, ObjectContent>,
}

#[derive(Serialize)]
struct ErrorOutput {
    error: String,
}

impl ReadObjects {
    pub fn read_objects(
        file_path: &str,
        queries: &[ObjectQuery],
    ) -> Result<(Lang, HashMap<String, ObjectContent>), Error> {
        let source_code = fs::read_to_string(file_path)
            .map_err(|e| Error::Io(format!("failed to read file: {}", e)))?;

        let mut parser = UniversalParser::new();
        let (lang, objects) = parser.parse_file(file_path).map_err(Error::Parse)?;

        let lines: Vec<&str> = source_code.lines().collect();
        let mut results = HashMap::new();

        for query in queries {
            for obj in &objects {
                if query.matches(obj) {
                    let content = Self::extract_content(obj, &lines);
                    results.insert(obj.name.clone(), content);
                }
            }
        }

        Ok((lang, results))
    }

    fn extract_content(obj: &ParsedObject, lines: &[&str]) -> ObjectContent {
        let start_idx = obj.line_start.saturating_sub(1);
        let end_idx = obj.line_end.min(lines.len());

        let content = lines[start_idx..end_idx].join("\n");

        ObjectContent {
            kind: obj.kind.name().to_string(),
            line_start: obj.line_start,
            line_end: obj.line_end,
            content,
        }
    }

    fn parse_input(input: &ToolInput) -> Result<(String, Vec<ObjectQuery>), Error> {
        let full_path_to_file = input
            .require_text("full_path_to_file")
            .map_err(|e| Error::Parse(e))?;

        // Parse queries - they should be in <queries><query>...</query></queries>
        let query_elements = input.get_elements("query");
        if query_elements.is_empty() {
            return Err(Error::Parse("at least one query is required".into()));
        }

        let mut queries = Vec::new();
        for query_elem in query_elements {
            let name = query_elem
                .get_text("name")
                .ok_or_else(|| Error::Parse("query must have a name".into()))?;
            let kind_str = query_elem.get_text("kind");

            let query = match kind_str {
                Some(kind) => ObjectQuery {
                    kind: ObjectKind::from_str(&kind),
                    name,
                },
                None => ObjectQuery::new(&name),
            };
            queries.push(query);
        }

        Ok((full_path_to_file, queries))
    }

    fn format_output(lang: Lang, results: HashMap<String, ObjectContent>) -> String {
        // Format as simple text output instead of YAML
        if results.is_empty() {
            return format!("Language: {}\nNo objects found.", lang.name());
        }

        let mut output = format!("Language: {}\nObjects found:\n", lang.name());
        for (name, content) in results {
            output.push_str(&format!("\n{} ({}):\n", name, content.kind));
            output.push_str(&format!(
                "Lines {}-{}:\n",
                content.line_start, content.line_end
            ));
            output.push_str(&content.content);
            output.push_str("\n");
        }
        output
    }
}

impl Tool for ReadObjects {
    fn name(&self) -> &'static str {
        "read_objects"
    }

    fn work(&self, input: &ToolInput, _request: &dyn Request) -> ToolResult {
        match Self::parse_input(input) {
            Ok((full_path_to_file, queries)) => {
                match Self::read_objects(&full_path_to_file, &queries) {
                    Ok((lang, results)) => {
                        ToolResult::ok(self.name(), input, Self::format_output(lang, results))
                    }
                    Err(e) => ToolResult::error(self.name(), input, e.to_string()),
                }
            }
            Err(e) => ToolResult::error(self.name(), input, e.to_string()),
        }
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{}` tool to read source code of specific objects from a file. Fill the input format precisely:

<tool_name>{}</tool_name>
<input>
  <full_path_to_file>string</full_path_to_file>  # path to the source file
  <queries>
    <query>
      <name>string</name>           # object name to find
      <kind>string</kind>           # optional: function, class, struct, etc.
    </query>
  </queries>
</input>"#,
            self.name(),
            self.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_rust_objects() {
        let source = r#"
pub struct MyStruct {
    field: i32,
}

impl MyStruct {
    pub fn new() -> Self {
        Self { field: 0 }
    }

    pub fn get_field(&self) -> i32 {
        self.field
    }
}

pub fn standalone_fn() -> i32 {
    42
}
"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, source).unwrap();

        let queries = vec![
            ObjectQuery::with_kind(ObjectKind::Struct, "MyStruct"),
            ObjectQuery::with_kind(ObjectKind::Function, "new"),
            ObjectQuery::with_kind(ObjectKind::Function, "standalone_fn"),
        ];

        let (lang, results) =
            ReadObjects::read_objects(file_path.to_str().unwrap(), &queries).unwrap();

        assert_eq!(lang, Lang::Rust);
        assert_eq!(results.len(), 3);

        let struct_result = results.get("MyStruct").unwrap();
        assert_eq!(struct_result.kind, "struct");
        assert!(struct_result.content.contains("pub struct MyStruct"));

        let new_result = results.get("new").unwrap();
        assert_eq!(new_result.kind, "function");
    }

    #[test]
    fn test_read_python_objects() {
        let source = r#"
def hello():
    print("Hello")

class MyClass:
    def method(self):
        pass
"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.py");
        std::fs::write(&file_path, source).unwrap();

        let queries = vec![
            ObjectQuery::new("hello"),
            ObjectQuery::with_kind(ObjectKind::Class, "MyClass"),
        ];

        let (lang, results) =
            ReadObjects::read_objects(file_path.to_str().unwrap(), &queries).unwrap();

        assert_eq!(lang, Lang::Python);
        assert!(results.contains_key("hello"));
        assert!(results.contains_key("MyClass"));
    }

    #[test]
    fn test_parse_input() {
        use crate::domain::tools::ToolInput;
        let input_xml = r#"<input>
  <full_path_to_file>/path/to/file.rs</full_path_to_file>
  <queries>
    <query>
      <name>main</name>
      <kind>function</kind>
    </query>
    <query>
      <name>Config</name>
      <kind>struct</kind>
    </query>
    <query>
      <name>Parser</name>
    </query>
  </queries>
</input>"#;
        let input = ToolInput::new(input_xml).unwrap();
        let (file_path, queries) = ReadObjects::parse_input(&input).unwrap();

        assert_eq!(file_path, "/path/to/file.rs");
        assert_eq!(queries.len(), 3);
        assert_eq!(queries[0].name, "main");
        assert_eq!(queries[1].name, "Config");
        assert_eq!(queries[2].name, "Parser");
    }

    #[test]
    fn test_query_matching() {
        let obj = ParsedObject {
            name: "my_function".to_string(),
            kind: ObjectKind::Function,
            line_start: 1,
            line_end: 5,
            byte_start: 0,
            byte_end: 100,
            visibility: None,
        };

        assert!(ObjectQuery::new("my_function").matches(&obj));
        assert!(ObjectQuery::with_kind(ObjectKind::Function, "my_function").matches(&obj));
        assert!(!ObjectQuery::with_kind(ObjectKind::Class, "my_function").matches(&obj));
        assert!(ObjectQuery::new("function").matches(&obj));
    }
}
