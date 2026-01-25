use super::{short_filename, Error, Tool, ToolResult, TOOL_OUTPUT_BUDGET_CHARS};
use crate::domain::session::Request;
use crate::utils::{Lang, ObjectKind, ParsedObject, UniversalParser};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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
    query: String,
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

    fn matches(&self, obj: &ParsedObject) -> bool {
        let name_matches = obj.name == self.name || obj.name.contains(&self.name);

        match self.kind {
            Some(kind) => kind == obj.kind && name_matches,
            None => name_matches,
        }
    }
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

        let mut names = Self::parse_query(&parsed.query);
        names.retain(|name| !name.trim().is_empty());
        if names.is_empty() {
            return Err(Error::Parse("query is required".into()));
        }

        let mut queries = Vec::new();
        for name in names {
            if name.trim().is_empty() {
                return Err(Error::Parse("names cannot include empty strings".into()));
            }
            queries.push(ObjectQuery::new(&name));
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

    fn parse_query(query: &str) -> Vec<String> {
        query
            .replace(',', " ")
            .split_whitespace()
            .map(|part| part.trim().to_string())
            .collect()
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
                return ToolResult::error(self.name().to_string(), String::new(), e.to_string())
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
                "query": {
                    "type": "string",
                    "description": "comma- or space-separated object names. Example: \"main, Config, Parser\""
                }
            },
            "required": ["path", "query"],
            "additionalProperties": false
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use the `{}` tool to read source code of specific objects from a file. To determine correct properties to use for `{}`, use the `discover_objects` tool first.",
            self.name(), self.name()
        )
    }

    fn get_output_budget(&self) -> Option<usize> {
        Some(TOOL_OUTPUT_BUDGET_CHARS)
    }

    fn get_input(&self) -> String {
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .map(|input| input.raw.clone())
            .unwrap_or_default()
    }

    fn get_progress_message(&self, _request: &dyn Request) -> String {
        match self.load_input() {
            Ok(input) => format!("Reading {}", short_filename(&input.full_path_to_file)),
            Err(_) => "Reading files".to_string(),
        }
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

impl Default for ReadObjects {
    fn default() -> Self {
        Self::new()
    }
}
