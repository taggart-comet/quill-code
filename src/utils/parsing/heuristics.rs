use regex::Regex;
use std::path::Path;

use super::tree_sitter::ObjectKind;

#[derive(Debug, Clone)]
pub struct HeuristicObject {
    pub name: String,
    pub kind: ObjectKind,
    pub line_start: usize,
    pub line_end: usize,
    pub indent_level: usize,
}

#[derive(Debug, Clone)]
pub struct LanguagePatterns {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub patterns: &'static [(&'static str, ObjectKind)],
    pub block_start: Option<&'static str>,
    pub block_end: Option<&'static str>,
    pub indent_based: bool,
}

pub struct HeuristicParser {
    language_patterns: Vec<LanguagePatterns>,
}

impl HeuristicParser {
    pub fn new() -> Self {
        Self {
            language_patterns: Self::default_patterns(),
        }
    }

    fn default_patterns() -> Vec<LanguagePatterns> {
        vec![
            // PHP
            LanguagePatterns {
                name: "PHP",
                extensions: &["php", "phtml", "php3", "php4", "php5", "phps"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:public|private|protected|static|\s)*\s*function\s+(\w+)\s*\(",
                        ObjectKind::Function,
                    ),
                    (r"(?m)^\s*(?:abstract\s+)?class\s+(\w+)", ObjectKind::Class),
                    (r"(?m)^\s*interface\s+(\w+)", ObjectKind::Interface),
                    (r"(?m)^\s*trait\s+(\w+)", ObjectKind::Trait),
                    (r"(?m)^\s*(?:final\s+)?enum\s+(\w+)", ObjectKind::Enum),
                    (r"(?m)^\s*const\s+(\w+)\s*=", ObjectKind::Constant),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Kotlin
            LanguagePatterns {
                name: "Kotlin",
                extensions: &["kt", "kts"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|inline|suspend|\s)*\s*fun\s+(?:<[^>]+>\s*)?(\w+)\s*\(",
                        ObjectKind::Function,
                    ),
                    (
                        r"(?m)^\s*(?:data\s+|sealed\s+|abstract\s+|open\s+)?class\s+(\w+)",
                        ObjectKind::Class,
                    ),
                    (
                        r"(?m)^\s*(?:fun\s+)?interface\s+(\w+)",
                        ObjectKind::Interface,
                    ),
                    (r"(?m)^\s*object\s+(\w+)", ObjectKind::Module),
                    (r"(?m)^\s*enum\s+class\s+(\w+)", ObjectKind::Enum),
                    (
                        r"(?m)^\s*(?:const\s+)?val\s+(\w+)\s*[=:]",
                        ObjectKind::Constant,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // C#
            LanguagePatterns {
                name: "C#",
                extensions: &["cs"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|static|async|virtual|override|abstract|\s)*\s*(?:\w+(?:<[^>]+>)?)\s+(\w+)\s*\([^)]*\)\s*(?:where\s+\w+\s*:\s*\w+\s*)?[{]?",
                        ObjectKind::Function,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|static|abstract|sealed|partial|\s)*\s*class\s+(\w+)",
                        ObjectKind::Class,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|\s)*\s*interface\s+(\w+)",
                        ObjectKind::Interface,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|\s)*\s*struct\s+(\w+)",
                        ObjectKind::Struct,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|\s)*\s*enum\s+(\w+)",
                        ObjectKind::Enum,
                    ),
                    (r"(?m)^\s*namespace\s+([\w.]+)", ObjectKind::Module),
                    (
                        r"(?m)^\s*(?:public|private|protected|internal|\s)*\s*const\s+\w+\s+(\w+)\s*=",
                        ObjectKind::Constant,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Swift
            LanguagePatterns {
                name: "Swift",
                extensions: &["swift"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:@\w+\s+)*(?:public|private|internal|fileprivate|open|static|class|override|\s)*\s*func\s+(\w+)\s*[<(]",
                        ObjectKind::Function,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|internal|fileprivate|open|final|\s)*\s*class\s+(\w+)",
                        ObjectKind::Class,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|internal|fileprivate|\s)*\s*struct\s+(\w+)",
                        ObjectKind::Struct,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|internal|fileprivate|\s)*\s*enum\s+(\w+)",
                        ObjectKind::Enum,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|internal|fileprivate|\s)*\s*protocol\s+(\w+)",
                        ObjectKind::Interface,
                    ),
                    (
                        r"(?m)^\s*(?:public|private|internal|fileprivate|\s)*\s*extension\s+(\w+)",
                        ObjectKind::Class,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Scala
            LanguagePatterns {
                name: "Scala",
                extensions: &["scala", "sc"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:override\s+)?(?:private|protected|\s)*\s*def\s+(\w+)\s*[(\[]",
                        ObjectKind::Function,
                    ),
                    (r"(?m)^\s*(?:case\s+)?class\s+(\w+)", ObjectKind::Class),
                    (r"(?m)^\s*trait\s+(\w+)", ObjectKind::Trait),
                    (r"(?m)^\s*object\s+(\w+)", ObjectKind::Module),
                    (r"(?m)^\s*(?:sealed\s+)?enum\s+(\w+)", ObjectKind::Enum),
                    (r"(?m)^\s*val\s+(\w+)\s*[=:]", ObjectKind::Constant),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Lua
            LanguagePatterns {
                name: "Lua",
                extensions: &["lua"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:local\s+)?function\s+(\w+(?:\.\w+)*)\s*\(",
                        ObjectKind::Function,
                    ),
                    (r"(?m)^\s*(\w+)\s*=\s*function\s*\(", ObjectKind::Function),
                    (r"(?m)^\s*local\s+(\w+)\s*=\s*\{", ObjectKind::Module),
                ],
                block_start: None,
                block_end: Some("end"),
                indent_based: false,
            },
            // Perl
            LanguagePatterns {
                name: "Perl",
                extensions: &["pl", "pm", "t"],
                patterns: &[
                    (r"(?m)^\s*sub\s+(\w+)\s*[{(]?", ObjectKind::Function),
                    (r"(?m)^\s*package\s+([\w:]+)", ObjectKind::Module),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // R
            LanguagePatterns {
                name: "R",
                extensions: &["r", "R", "rmd", "Rmd"],
                patterns: &[
                    (r"(?m)^\s*(\w+)\s*<-\s*function\s*\(", ObjectKind::Function),
                    (r"(?m)^\s*(\w+)\s*=\s*function\s*\(", ObjectKind::Function),
                    (
                        r#"(?m)^\s*setClass\s*\(\s*["'](\w+)["']"#,
                        ObjectKind::Class,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Haskell
            LanguagePatterns {
                name: "Haskell",
                extensions: &["hs", "lhs"],
                patterns: &[
                    (r"(?m)^(\w+)\s*::\s*", ObjectKind::Function),
                    (r"(?m)^\s*data\s+(\w+)", ObjectKind::Struct),
                    (r"(?m)^\s*newtype\s+(\w+)", ObjectKind::Struct),
                    (r"(?m)^\s*class\s+(\w+)", ObjectKind::Class),
                    (r"(?m)^\s*instance\s+(\w+)", ObjectKind::Class),
                    (r"(?m)^\s*module\s+([\w.]+)", ObjectKind::Module),
                ],
                block_start: None,
                block_end: None,
                indent_based: true,
            },
            // Elixir
            LanguagePatterns {
                name: "Elixir",
                extensions: &["ex", "exs"],
                patterns: &[
                    (r"(?m)^\s*def\s+(\w+)[(\s]", ObjectKind::Function),
                    (r"(?m)^\s*defp\s+(\w+)[(\s]", ObjectKind::Function),
                    (r"(?m)^\s*defmodule\s+([\w.]+)", ObjectKind::Module),
                    (r"(?m)^\s*defmacro\s+(\w+)", ObjectKind::Function),
                    (r"(?m)^\s*defstruct\s+", ObjectKind::Struct),
                ],
                block_start: Some("do"),
                block_end: Some("end"),
                indent_based: false,
            },
            // Clojure
            LanguagePatterns {
                name: "Clojure",
                extensions: &["clj", "cljs", "cljc", "edn"],
                patterns: &[
                    (r"(?m)^\s*\(defn-?\s+(\w+)", ObjectKind::Function),
                    (r"(?m)^\s*\(defmacro\s+(\w+)", ObjectKind::Function),
                    (r"(?m)^\s*\(defrecord\s+(\w+)", ObjectKind::Struct),
                    (r"(?m)^\s*\(defprotocol\s+(\w+)", ObjectKind::Interface),
                    (r"(?m)^\s*\(ns\s+([\w.-]+)", ObjectKind::Module),
                ],
                block_start: Some("("),
                block_end: Some(")"),
                indent_based: false,
            },
            // OCaml
            LanguagePatterns {
                name: "OCaml",
                extensions: &["ml", "mli"],
                patterns: &[
                    (
                        r"(?m)^\s*let\s+(?:rec\s+)?(\w+)\s*[=:]",
                        ObjectKind::Function,
                    ),
                    (r"(?m)^\s*type\s+(\w+)", ObjectKind::Struct),
                    (r"(?m)^\s*module\s+(\w+)", ObjectKind::Module),
                    (r"(?m)^\s*class\s+(\w+)", ObjectKind::Class),
                ],
                block_start: None,
                block_end: None,
                indent_based: false,
            },
            // Zig
            LanguagePatterns {
                name: "Zig",
                extensions: &["zig"],
                patterns: &[
                    (r"(?m)^\s*(?:pub\s+)?fn\s+(\w+)\s*\(", ObjectKind::Function),
                    (
                        r"(?m)^\s*(?:pub\s+)?const\s+(\w+)\s*=\s*struct",
                        ObjectKind::Struct,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+)?const\s+(\w+)\s*=\s*enum",
                        ObjectKind::Enum,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+)?const\s+(\w+)\s*=",
                        ObjectKind::Constant,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Dart
            LanguagePatterns {
                name: "Dart",
                extensions: &["dart"],
                patterns: &[
                    (
                        r"(?m)^\s*(?:static\s+)?(?:\w+(?:<[^>]+>)?)\s+(\w+)\s*\([^)]*\)\s*(?:async\s*)?[{]",
                        ObjectKind::Function,
                    ),
                    (r"(?m)^\s*(?:abstract\s+)?class\s+(\w+)", ObjectKind::Class),
                    (r"(?m)^\s*enum\s+(\w+)", ObjectKind::Enum),
                    (r"(?m)^\s*mixin\s+(\w+)", ObjectKind::Trait),
                    (r"(?m)^\s*extension\s+(\w+)", ObjectKind::Class),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
            // Generic fallback for unknown languages
            LanguagePatterns {
                name: "Unknown",
                extensions: &[],
                patterns: &[
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+|private\s+|protected\s+)?(?:static\s+)?(?:async\s+)?(?:fn|func|function|def|sub|proc)\s+(\w+)\s*[(<]",
                        ObjectKind::Function,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+)?(?:abstract\s+)?(?:final\s+)?class\s+(\w+)",
                        ObjectKind::Class,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+)?struct\s+(\w+)",
                        ObjectKind::Struct,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+)?enum\s+(\w+)",
                        ObjectKind::Enum,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+)?(?:interface|protocol|trait)\s+(\w+)",
                        ObjectKind::Interface,
                    ),
                    (
                        r"(?m)^\s*(?:pub\s+|public\s+)?module\s+(\w+)",
                        ObjectKind::Module,
                    ),
                ],
                block_start: Some("{"),
                block_end: Some("}"),
                indent_based: false,
            },
        ]
    }

    pub fn detect_language(&self, path: &str) -> Option<&LanguagePatterns> {
        let ext = Path::new(path).extension().and_then(|e| e.to_str())?;

        self.language_patterns
            .iter()
            .find(|lang| lang.extensions.contains(&ext))
    }

    pub fn get_fallback(&self) -> &LanguagePatterns {
        self.language_patterns.last().unwrap()
    }

    pub fn parse(&self, source: &str, lang: &LanguagePatterns) -> Vec<HeuristicObject> {
        let mut objects = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        for (pattern_str, kind) in lang.patterns {
            if let Ok(regex) = Regex::new(pattern_str) {
                for cap in regex.captures_iter(source) {
                    if let Some(name_match) = cap.get(1) {
                        let byte_start = name_match.start();
                        let line_start = source[..byte_start].matches('\n').count() + 1;

                        let line_end = self.find_block_end(&lines, line_start, lang);

                        let indent_level = lines
                            .get(line_start.saturating_sub(1))
                            .map(|l| l.len() - l.trim_start().len())
                            .unwrap_or(0);

                        objects.push(HeuristicObject {
                            name: name_match.as_str().to_string(),
                            kind: *kind,
                            line_start,
                            line_end,
                            indent_level,
                        });
                    }
                }
            }
        }

        objects.sort_by_key(|o| o.line_start);
        objects.dedup_by(|a, b| a.line_start == b.line_start && a.name == b.name);

        objects
    }

    fn find_block_end(&self, lines: &[&str], start_line: usize, lang: &LanguagePatterns) -> usize {
        let start_idx = start_line.saturating_sub(1);

        if start_idx >= lines.len() {
            return start_line;
        }

        if lang.indent_based {
            return self.find_indent_block_end(lines, start_idx);
        }

        let (block_start, block_end) = match (lang.block_start, lang.block_end) {
            (Some(s), Some(e)) => (s, e),
            (None, Some(e)) => ("", e),
            _ => return self.find_indent_block_end(lines, start_idx),
        };

        let mut depth = 0;
        let mut found_start = block_start.is_empty();

        for (i, line) in lines.iter().enumerate().skip(start_idx) {
            if !found_start && line.contains(block_start) {
                found_start = true;
                depth = 1;
                continue;
            }

            if found_start {
                for ch in line.chars() {
                    if !block_start.is_empty() && block_start.starts_with(ch) {
                        depth += 1;
                    } else if block_end.starts_with(ch) {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                }

                if block_end.len() > 1 && line.trim() == block_end {
                    depth -= 1;
                    if depth <= 0 {
                        return i + 1;
                    }
                }
            }
        }

        lines.len()
    }

    fn find_indent_block_end(&self, lines: &[&str], start_idx: usize) -> usize {
        if start_idx >= lines.len() {
            return start_idx + 1;
        }

        let start_indent = lines[start_idx].len() - lines[start_idx].trim_start().len();

        for (i, line) in lines.iter().enumerate().skip(start_idx + 1) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let current_indent = line.len() - trimmed.len();
            if current_indent <= start_indent {
                return i;
            }
        }

        lines.len()
    }

    pub fn parse_file(&self, path: &str) -> Result<(String, Vec<HeuristicObject>), String> {
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

        let lang = self
            .detect_language(path)
            .unwrap_or_else(|| self.get_fallback());
        let objects = self.parse(&source, lang);

        Ok((lang.name.to_string(), objects))
    }
}

impl Default for HeuristicParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_php_parsing() {
        let source = r#"
<?php
class MyClass {
    public function hello() {
        echo "Hello";
    }

    private function world() {
        echo "World";
    }
}

function standalone() {
    return 42;
}
"#;
        let parser = HeuristicParser::new();
        let lang = parser.detect_language("test.php").unwrap();
        let objects = parser.parse(source, lang);

        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "world"));
        assert!(objects.iter().any(|o| o.name == "standalone"));
    }

    #[test]
    fn test_kotlin_parsing() {
        let source = r#"
class MyClass {
    fun hello(): String {
        return "Hello"
    }

    suspend fun asyncMethod() {
        delay(1000)
    }
}

fun topLevel() = 42

data class User(val name: String)
"#;
        let parser = HeuristicParser::new();
        let lang = parser.detect_language("test.kt").unwrap();
        let objects = parser.parse(source, lang);

        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "asyncMethod"));
        assert!(objects.iter().any(|o| o.name == "topLevel"));
        assert!(objects.iter().any(|o| o.name == "User"));
    }

    #[test]
    fn test_csharp_parsing() {
        let source = r#"
namespace MyApp {
    public class MyClass {
        public void Hello() {
            Console.WriteLine("Hello");
        }

        private async Task<int> FetchData() {
            return await Task.FromResult(42);
        }
    }

    public interface IService {
        void DoWork();
    }

    public enum Status {
        Active,
        Inactive
    }
}
"#;
        let parser = HeuristicParser::new();
        let lang = parser.detect_language("test.cs").unwrap();
        let objects = parser.parse(source, lang);

        assert!(objects.iter().any(|o| o.name == "MyApp"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "IService"));
        assert!(objects.iter().any(|o| o.name == "Status"));
    }

    #[test]
    fn test_swift_parsing() {
        let source = r#"
class MyClass {
    func hello() -> String {
        return "Hello"
    }

    @objc private func callback() {
    }
}

struct Point {
    var x: Int
    var y: Int
}

protocol Drawable {
    func draw()
}
"#;
        let parser = HeuristicParser::new();
        let lang = parser.detect_language("test.swift").unwrap();
        let objects = parser.parse(source, lang);

        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Point"));
        assert!(objects.iter().any(|o| o.name == "Drawable"));
    }

    #[test]
    fn test_fallback_for_unknown() {
        let source = r#"
function hello() {
    print("hello")
}

class Foo {
}
"#;
        let parser = HeuristicParser::new();
        let lang = parser.detect_language("test.xyz");
        assert!(lang.is_none());

        let fallback = parser.get_fallback();
        let objects = parser.parse(source, fallback);

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Foo"));
    }
}
