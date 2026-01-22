use crate::domain::session::Request;
use crate::domain::tools::{Error, Tool, ToolResult};
use crate::utils::{Lang, ParsedObject, UniversalParser};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct DiscoverObjects {
    input: Mutex<Option<DiscoverObjectsInput>>,
}

#[derive(Debug, Clone)]
struct DiscoverObjectsInput {
    raw: String,
    full_path_to_file: String,
}

#[derive(Debug, Deserialize)]
struct DiscoverObjectsInputJson {
    full_path_to_file: String,
}

impl DiscoverObjects {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }

    pub fn parse_file(file_path: &str) -> Result<(Lang, Vec<ParsedObject>), String> {
        let mut parser = UniversalParser::new();
        parser.parse_file(file_path)
    }

    fn format_output(lang: Lang, objects: &[ParsedObject]) -> String {
        let mut output = String::new();
        output.push_str(&format!("language: {}\n", lang.name()));
        output.push_str("objects:\n");

        let mut by_kind: BTreeMap<&str, Vec<&ParsedObject>> = BTreeMap::new();
        for obj in objects {
            by_kind.entry(obj.kind.name()).or_default().push(obj);
        }

        for (kind, objs) in by_kind {
            output.push_str(&format!("  {}:\n", kind));
            for obj in objs {
                output.push_str(&format!(
                    "    - name: \"{}\"\n      lines: {}-{}\n",
                    obj.name, obj.line_start, obj.line_end
                ));
                if let Some(ref vis) = obj.visibility {
                    output.push_str(&format!("      visibility: {}\n", vis));
                }
            }
        }

        output
    }

    fn parse_input_json(raw: &str) -> Result<DiscoverObjectsInput, Error> {
        let parsed: DiscoverObjectsInputJson =
            serde_json::from_str(raw).map_err(|e| Error::Parse(e.to_string()))?;
        if parsed.full_path_to_file.is_empty() {
            return Err(Error::Parse("full_path_to_file is required".into()));
        }
        Ok(DiscoverObjectsInput {
            raw: raw.to_string(),
            full_path_to_file: parsed.full_path_to_file,
        })
    }

    fn load_input(&self) -> Result<DiscoverObjectsInput, Error> {
        self.input
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Parse("input not parsed".into()))
    }
}

impl Tool for DiscoverObjects {
    fn name(&self) -> &'static str {
        "discover_objects"
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
                return ToolResult::error(self.name().to_string(), String::new(), e.to_string())
            }
        };

        match Self::parse_file(&input.full_path_to_file) {
            Ok((lang, objects)) => ToolResult::ok(
                self.name().to_string(),
                input.raw,
                Self::format_output(lang, &objects),
            ),
            Err(e) => ToolResult::error(self.name().to_string(), input.raw, e.to_string()),
        }
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "full_path_to_file": {
                    "type": "string",
                    "description": "path to the source file, use `find_files` tool to determine the correct path to the file"
                }
            },
            "required": ["full_path_to_file"],
            "additionalProperties": false
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use the `{}` tool to discover exact names for functions, classes, structs in a source file.",
            self.name()
        )
    }

    fn get_input(&self) -> String {
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .map(|input| input.raw.clone())
            .unwrap_or_default()
    }

    fn get_affected_paths(&self, request: &dyn Request) -> Vec<PathBuf> {
        match self.input.lock().unwrap().as_ref() {
            Some(input) => {
                let path = PathBuf::from(&input.full_path_to_file);
                if path.is_absolute() {
                    vec![path]
                } else {
                    vec![request.project_root().join(path)]
                }
            }
            None => vec![],
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Default for DiscoverObjects {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_rust_source() {
        let source = r#"
pub struct MyStruct {
    field: i32,
}

enum MyEnum {
    Variant1,
    Variant2,
}

pub trait MyTrait {
    fn method(&self);
}

impl MyTrait for MyStruct {
    fn method(&self) {}
}

impl MyStruct {
    pub fn new() -> Self {
        Self { field: 0 }
    }
}

pub const MAX_SIZE: usize = 100;

fn private_function() {}

 pub fn public_function(x: i32) -> i32 {
    x * 2
 }
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.rs");
        fs::write(&file_path, source).unwrap();
        let (_lang, objects) = DiscoverObjects::parse_file(file_path.to_str().unwrap()).unwrap();

        assert!(objects.iter().any(|o| o.name == "MyStruct"));
        assert!(objects.iter().any(|o| o.name == "MyEnum"));
        assert!(objects.iter().any(|o| o.name == "MyTrait"));
        assert!(objects.iter().any(|o| o.name == "MAX_SIZE"));
        assert!(objects.iter().any(|o| o.name == "private_function"));
        assert!(objects.iter().any(|o| o.name == "public_function"));
    }

    #[test]
    fn test_parse_python_source() {
        let source = r#"
 def hello():
    pass

class MyClass:
    def __init__(self):
        pass

    def method(self):
        pass

 async def async_func():
    pass
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.py");
        fs::write(&file_path, source).unwrap();
        let (_lang, objects) = DiscoverObjects::parse_file(file_path.to_str().unwrap()).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "__init__"));
        assert!(objects.iter().any(|o| o.name == "method"));
        assert!(objects.iter().any(|o| o.name == "async_func"));
    }

    #[test]
    fn test_parse_javascript_source() {
        let source = r#"
 function hello() {
    console.log("Hello");
}

class MyClass {
    constructor() {}

    method() {}
}

 const arrowFn = () => {};
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.js");
        fs::write(&file_path, source).unwrap();
        let (_lang, objects) = DiscoverObjects::parse_file(file_path.to_str().unwrap()).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
    }

    #[test]
    fn test_parse_go_source() {
        let source = r#"
 package main

func hello() {
    fmt.Println("Hello")
}

type Person struct {
    Name string
    Age  int
}

 func (p *Person) Greet() string {
    return "Hello, " + p.Name
 }
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.go");
        fs::write(&file_path, source).unwrap();
        let (_lang, objects) = DiscoverObjects::parse_file(file_path.to_str().unwrap()).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Greet"));
    }
}
