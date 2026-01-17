use super::{Error, Tool, ToolResult};
use crate::domain::session::Request;
use crate::utils::{Lang, ObjectKind, ParsedObject, UniversalParser};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

pub struct ReadObjects {
    input: Mutex<Option<ReadObjectsInput>>,
}

#[derive(Debug, Clone)]
struct ReadObjectsInput {
    raw: String,
    full_path_to_file: String,
    queries: Vec<ObjectQuery>,
}

#[derive(Debug, Deserialize)]
struct ReadObjectsInputJson {
    path: String,
    names: Vec<String>,
    kind: Option<String>,
}

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
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }

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

    fn parse_input_json(raw: &str) -> Result<ReadObjectsInput, Error> {
        let parsed: ReadObjectsInputJson =
            serde_json::from_str(raw).map_err(|e| Error::Parse(e.to_string()))?;
        if parsed.path.is_empty() {
            return Err(Error::Parse("path is required".into()));
        }

        if parsed.names.is_empty() {
            return Err(Error::Parse("names is required".into()));
        }

        let kind = parsed.kind.as_deref().and_then(ObjectKind::from_str);
        if parsed.kind.is_some() && kind.is_none() {
            return Err(Error::Parse("invalid kind".into()));
        }

        let mut queries = Vec::new();
        for name in parsed.names {
            if name.is_empty() {
                return Err(Error::Parse("names cannot include empty strings".into()));
            }
            let query = match kind {
                Some(kind) => ObjectQuery::with_kind(kind, &name),
                None => ObjectQuery::new(&name),
            };
            queries.push(query);
        }

        Ok(ReadObjectsInput {
            raw: raw.to_string(),
            full_path_to_file: parsed.path,
            queries,
        })
    }

    fn load_input(&self) -> Result<ReadObjectsInput, Error> {
        self.input
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Parse("input not parsed".into()))
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

    fn parse_input(&self, input: String) -> Option<Error> {
        let trimmed = input.trim();
        let parsed = Self::parse_input_json(trimmed);

        match parsed {
            Ok(parsed) => {
                *self.input.lock().unwrap() = Some(parsed);
                None
            }
            Err(err) => Some(err),
        }
    }

    fn work(&self, _request: &dyn Request) -> ToolResult {
        let input = match self.load_input() {
            Ok(input) => input,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    String::new(),
                    e.to_string(),
                )
            }
        };

        match Self::read_objects(&input.full_path_to_file, &input.queries) {
            Ok((lang, results)) => ToolResult::ok(
                self.name().to_string(),
                input.raw,
                Self::format_output(lang, results),
            ),
            Err(e) => ToolResult::error(self.name().to_string(), input.raw, e.to_string()),
        }
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "path to the source file"
                },
                "names": {
                    "type": "array",
                    "description": "object names to find",
                    "items": {
                        "type": "string"
                    }
                },
                "kind": {
                    "type": "string",
                    "description": "optional: apply one kind to all names (function, class, struct, etc.)"
                }
            },
            "required": ["path", "names"],
            "additionalProperties": false
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use the `{}` tool to read source code of specific objects from a file.",
            self.name()
        )
    }

}

impl Default for ReadObjects {
    fn default() -> Self {
        Self::new()
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
        let source = r#"
pub fn main() {}
pub struct Config {}
pub struct Parser {}
"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, source).unwrap();

        let input_json = format!(
            r#"{{"path":"{}","names":["main","Config","Parser"]}}"#,
            file_path.display()
        );

        let tool = ReadObjects::new();
        assert!(tool.parse_input(input_json).is_none());
        let result = tool.work(&crate::domain::session::VirtualRequest::new(
            "test",
            temp_dir.path(),
        ));

        let output = result.output_string();
        assert!(output.contains("main"));
        assert!(output.contains("Config"));
        assert!(output.contains("Parser"));
    }

    #[test]
    fn test_parse_input_invalid_kind() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn main() {}").unwrap();

        let input_json = format!(
            r#"{{"path":"{}","names":["main"],"kind":"not_a_kind"}}"#,
            file_path.display()
        );

        let tool = ReadObjects::new();
        let err = tool.parse_input(input_json).unwrap();
        assert!(err.to_string().contains("invalid kind"));
    }

    #[test]
    fn test_parse_input_missing_names() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn main() {}").unwrap();

        let input_json = format!(r#"{{"path":"{}","names":[]}}"#, file_path.display());

        let tool = ReadObjects::new();
        let err = tool.parse_input(input_json).unwrap();
        assert!(err.to_string().contains("names is required"));
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
