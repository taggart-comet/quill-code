use super::{Error, Tool, ToolResult};
use crate::utils::{Lang, ObjectKind, ParsedObject, UniversalParser};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as Yaml;
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

#[derive(Deserialize)]
struct ReadObjectsInput {
    full_path_to_file: String,
    queries: Vec<QueryInput>,
}

#[derive(Deserialize)]
struct QueryInput {
    #[serde(default)]
    kind: Option<String>,
    name: String,
}

#[derive(Serialize)]
struct ReadObjectsOutput {
    language: String,
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
        let source_code =
            fs::read_to_string(file_path).map_err(|e| Error::Io(format!("failed to read file: {}", e)))?;

        let mut parser = UniversalParser::new();
        let (lang, objects) = parser.parse_file(file_path)
            .map_err(Error::Parse)?;

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

    fn parse_input(input: Yaml) -> Result<ReadObjectsInput, Error> {
        let parsed: ReadObjectsInput =
            serde_yaml::from_value(input).map_err(|e| Error::InvalidYaml(e.to_string()))?;

        if parsed.queries.is_empty() {
            return Err(Error::Parse("at least one query is required".into()));
        }

        Ok(parsed)
    }

    fn format_output(lang: Lang, results: HashMap<String, ObjectContent>) -> String {
        let output = ReadObjectsOutput {
            language: lang.name().to_string(),
            results,
        };
        serde_yaml::to_string(&output).unwrap_or_else(|e| format!("error: {}", e))
    }

    fn format_error(error: impl Into<String>) -> String {
        serde_yaml::to_string(&ErrorOutput {
            error: error.into(),
        })
        .unwrap_or_else(|e| format!("error: {}", e))
    }
}

impl Tool for ReadObjects {
    fn name(&self) -> &'static str {
        "read_objects"
    }

    fn work(&self, input: Yaml) -> ToolResult {
        let input_copy = input.clone();
        
        match Self::parse_input(input) {
            Ok(parsed) => {
                let queries: Vec<ObjectQuery> = parsed
                    .queries
                    .into_iter()
                    .map(|q| match q.kind {
                        Some(kind_str) => ObjectQuery {
                            kind: ObjectKind::from_str(&kind_str),
                            name: q.name,
                        },
                        None => ObjectQuery::new(&q.name),
                    })
                    .collect();

                match Self::read_objects(&parsed.full_path_to_file, &queries) {
                    Ok((lang, results)) => ToolResult::ok(self.name(), input_copy, Yaml::String(Self::format_output(lang, results))),
                    Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
                }
            }
            Err(e) => ToolResult::error(self.name(), input_copy, e.to_string()),
        }
    }

    fn desc(&self) -> &'static str {
        "Shows code for requested objects in a source file. Supports: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, Ruby, and more."
    }

    fn input_format(&self) -> &'static str {
        "
input:
  full_path_to_file: string
  queries:
    - name: string
      kind: string
"
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
        let input = r#"
full_path_to_file: /path/to/file.rs
queries:
  - name: main
    kind: function
  - name: Config
    kind: struct
  - name: Parser
"#;
        let yaml: Yaml = serde_yaml::from_str(input).unwrap();
        let parsed = ReadObjects::parse_input(yaml).unwrap();

        assert_eq!(parsed.full_path_to_file, "/path/to/file.rs");
        assert_eq!(parsed.queries.len(), 3);
        assert_eq!(parsed.queries[0].kind, Some("function".to_string()));
        assert_eq!(parsed.queries[1].kind, Some("struct".to_string()));
        assert_eq!(parsed.queries[2].kind, None);
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
