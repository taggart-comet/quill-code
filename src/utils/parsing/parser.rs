use super::heuristics::{HeuristicObject, HeuristicParser};
use super::tree_sitter::{Lang, ParsedObject, TreeSitterParser};

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub language: String,
    pub objects: Vec<ParsedObject>,
    pub used_heuristics: bool,
}

impl ParsedObject {
    pub fn from_heuristic(obj: &HeuristicObject) -> Self {
        Self {
            name: obj.name.clone(),
            kind: obj.kind,
            line_start: obj.line_start,
            line_end: obj.line_end,
            byte_start: 0,
            byte_end: 0,
            visibility: None,
        }
    }
}

/// Universal parser that tries Tree-sitter first and falls back to heuristics if needed
pub struct UniversalParser {
    tree_sitter_parser: TreeSitterParser,
    heuristic_parser: HeuristicParser,
}

impl UniversalParser {
    pub fn new() -> Self {
        Self {
            tree_sitter_parser: TreeSitterParser::new(),
            heuristic_parser: HeuristicParser::new(),
        }
    }

    /// Parse source code with a known language using Tree-sitter
    pub fn parse(&mut self, source: &str, lang: Lang) -> Result<Vec<ParsedObject>, String> {
        self.tree_sitter_parser.parse(source, lang)
    }

    /// Parse a file with a known language using Tree-sitter
    pub fn parse_file(&mut self, path: &str) -> Result<(Lang, Vec<ParsedObject>), String> {
        self.tree_sitter_parser.parse_file(path)
    }

    /// Parse a file, trying Tree-sitter first and falling back to heuristics if needed
    pub fn parse_file_with_fallback(&mut self, path: &str) -> Result<ParseResult, String> {
        // Try Tree-sitter first if the language is supported
        if TreeSitterParser::supports(path) {
            match self.tree_sitter_parser.parse_file(path) {
                Ok((lang, objects)) => {
                    return Ok(ParseResult {
                        language: lang.name().to_string(),
                        objects,
                        used_heuristics: false,
                    });
                }
                Err(_) => {
                    // Tree-sitter failed, fall back to heuristics
                }
            }
        }

        // Fall back to heuristics
        let (lang_name, heuristic_objects) = self.heuristic_parser.parse_file(path)?;
        let objects = heuristic_objects
            .iter()
            .map(ParsedObject::from_heuristic)
            .collect();

        Ok(ParseResult {
            language: lang_name,
            objects,
            used_heuristics: true,
        })
    }

    /// Parse source code with fallback, given a file path for language detection
    pub fn parse_source_with_fallback(
        &mut self,
        source: &str,
        path: &str,
    ) -> Result<ParseResult, String> {
        // Try Tree-sitter first if the language is supported
        if let Some(lang) = Lang::from_path(path) {
            match self.tree_sitter_parser.parse(source, lang) {
                Ok(objects) => {
                    return Ok(ParseResult {
                        language: lang.name().to_string(),
                        objects,
                        used_heuristics: false,
                    });
                }
                Err(_) => {
                    // Tree-sitter failed, fall back to heuristics
                }
            }
        }

        // Fall back to heuristics
        let lang_patterns = self
            .heuristic_parser
            .detect_language(path)
            .unwrap_or_else(|| self.heuristic_parser.get_fallback());

        let heuristic_objects = self.heuristic_parser.parse(source, lang_patterns);
        let objects = heuristic_objects
            .iter()
            .map(ParsedObject::from_heuristic)
            .collect();

        Ok(ParseResult {
            language: lang_patterns.name.to_string(),
            objects,
            used_heuristics: true,
        })
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

    #[test]
    fn test_parse_rust_with_tree_sitter() {
        let source = r#"
pub fn hello() {}
struct Point { x: i32, y: i32 }
impl Point {
    fn new() -> Self { Self { x: 0, y: 0 } }
}
"#;
        let mut parser = UniversalParser::new();
        let result = parser
            .parse_source_with_fallback(source, "test.rs")
            .unwrap();

        assert!(!result.used_heuristics);
        assert_eq!(result.language, "Rust");
        assert!(result.objects.iter().any(|o| o.name == "hello"));
        assert!(result.objects.iter().any(|o| o.name == "Point"));
        assert!(result.objects.iter().any(|o| o.name == "new"));
    }

    #[test]
    fn test_fallback_to_heuristics_php() {
        let source = r#"
<?php
class MyClass {
    public function hello() {
        echo "Hello";
    }
}

function standalone() {
    return 42;
}
"#;
        let mut parser = UniversalParser::new();
        let result = parser
            .parse_source_with_fallback(source, "test.php")
            .unwrap();

        assert!(result.used_heuristics);
        assert_eq!(result.language, "PHP");
        assert!(result.objects.iter().any(|o| o.name == "MyClass"));
        assert!(result.objects.iter().any(|o| o.name == "hello"));
        assert!(result.objects.iter().any(|o| o.name == "standalone"));
    }

    #[test]
    fn test_fallback_to_heuristics_kotlin() {
        let source = r#"
class MyClass {
    fun hello(): String {
        return "Hello"
    }
}

fun topLevel() = 42

data class User(val name: String)
"#;
        let mut parser = UniversalParser::new();
        let result = parser
            .parse_source_with_fallback(source, "test.kt")
            .unwrap();

        assert!(result.used_heuristics);
        assert_eq!(result.language, "Kotlin");
        assert!(result.objects.iter().any(|o| o.name == "MyClass"));
        assert!(result.objects.iter().any(|o| o.name == "hello"));
        assert!(result.objects.iter().any(|o| o.name == "topLevel"));
        assert!(result.objects.iter().any(|o| o.name == "User"));
    }
}
