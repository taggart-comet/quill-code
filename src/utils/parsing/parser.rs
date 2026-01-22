use super::tree_sitter::{Lang, ParsedObject, TreeSitterParser};

/// Universal parser that tries Tree-sitter first and falls back to heuristics if needed
pub struct UniversalParser {
    tree_sitter_parser: TreeSitterParser,
}

impl UniversalParser {
    pub fn new() -> Self {
        Self {
            tree_sitter_parser: TreeSitterParser::new(),
        }
    }

    /// Parse a file with a known language using Tree-sitter
    pub fn parse_file(&mut self, path: &str) -> Result<(Lang, Vec<ParsedObject>), String> {
        self.tree_sitter_parser.parse_file(path)
    }
}

impl Default for UniversalParser {
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
    fn test_parse_rust_with_tree_sitter() {
        let source = r#"
 pub fn hello() {}
 struct Point { x: i32, y: i32 }
 impl Point {
    fn new() -> Self { Self { x: 0, y: 0 } }
 }
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.rs");
        fs::write(&file_path, source).unwrap();
        let mut parser = UniversalParser::new();
        let (lang, objects) = parser.parse_file(file_path.to_str().unwrap()).unwrap();

        assert_eq!(lang.name(), "Rust");
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Point"));
        assert!(objects.iter().any(|o| o.name == "new"));
    }

    #[test]
    fn test_parse_python_with_tree_sitter() {
        let source = r#"
def hello():
    pass

class MyClass:
    def __init__(self):
        pass

    def method(self):
        pass
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.py");
        fs::write(&file_path, source).unwrap();
        let mut parser = UniversalParser::new();
        let (lang, objects) = parser.parse_file(file_path.to_str().unwrap()).unwrap();

        assert_eq!(lang.name(), "Python");
        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "__init__"));
    }

    #[test]
    fn test_parse_javascript_with_tree_sitter() {
        let source = r#"
function hello() {
    console.log("hi");
}

class MyClass {
    constructor() {}
    method() {}
}
 "#;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample.js");
        fs::write(&file_path, source).unwrap();
        let mut parser = UniversalParser::new();
        let (lang, objects) = parser.parse_file(file_path.to_str().unwrap()).unwrap();

        assert_eq!(lang.name(), "JavaScript");
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
    }
}
