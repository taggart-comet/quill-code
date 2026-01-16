use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolInput, ToolResult};
use crate::utils::{Lang, ParsedObject, UniversalParser};
use std::collections::BTreeMap;

pub struct DiscoverObjects;

impl DiscoverObjects {
    pub fn parse_file(file_path: &str) -> Result<(Lang, Vec<ParsedObject>), String> {
        let mut parser = UniversalParser::new();
        parser.parse_file(file_path)
    }

    fn parse_source(source: &str, lang: Lang) -> Result<Vec<ParsedObject>, String> {
        let mut parser = UniversalParser::new();
        parser.parse(source, lang)
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
}

impl Tool for DiscoverObjects {
    fn name(&self) -> &'static str {
        "discover_objects"
    }

    fn work(&self, input: &ToolInput, _request: &dyn Request) -> ToolResult {
        let full_path_to_file = match input.require_text("full_path_to_file") {
            Ok(path) => path,
            Err(e) => return ToolResult::error(self.name(), input, e),
        };

        if full_path_to_file.is_empty() {
            return ToolResult::error(
                self.name(),
                input,
                "full_path_to_file is required".to_string(),
            );
        }

        match Self::parse_file(&full_path_to_file) {
            Ok((lang, objects)) => {
                ToolResult::ok(self.name(), input, Self::format_output(lang, &objects))
            }
            Err(e) => ToolResult::error(self.name(), input, e.to_string()),
        }
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{}` tool to discover functions, classes, structs in a source file. Fill the input format precisely:

<tool_name>{}</tool_name>
<input>
  <full_path_to_file>string</full_path_to_file>  # path to the source file
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
        let objects = DiscoverObjects::parse_source(source, Lang::Rust).unwrap();

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
        let objects = DiscoverObjects::parse_source(source, Lang::Python).unwrap();

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
        let objects = DiscoverObjects::parse_source(source, Lang::JavaScript).unwrap();

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
        let objects = DiscoverObjects::parse_source(source, Lang::Go).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Greet"));
    }
}
