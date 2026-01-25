use crate::domain::session::Request;
use crate::domain::tools::{
    short_label_from_path, Error, Tool, ToolResult, TOOL_OUTPUT_BUDGET_CHARS,
};
use crate::utils::paths::is_within_root;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use walkdir::WalkDir;

pub struct FindFiles {
    input: Mutex<Option<FindFilesInput>>,
}

#[derive(Debug, Clone)]
struct FindFilesInput {
    raw: String,
    query: String,
    root: Option<String>,
    max_results: usize,
}

#[derive(Debug, Deserialize)]
struct FindFilesInputJson {
    query: String,
    root: Option<String>,
    max_results: Option<usize>,
}

impl FindFiles {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }

    /// Resolve the search root, defaulting to project_root if not specified
    fn resolve_search_root(
        input_root: Option<&str>,
        project_root: &Path,
    ) -> Result<PathBuf, String> {
        match input_root {
            Some(root) => {
                let root_path = Path::new(root);

                if is_within_root(root_path, project_root) {
                    Ok(root_path.to_path_buf())
                } else {
                    Err(format!(
                        "Search root '{}' is outside project root '{}'",
                        root,
                        project_root.display()
                    ))
                }
            }
            None => {
                // Use project root as default
                Ok(project_root.to_path_buf())
            }
        }
    }

    /// Score a file path based on how well it matches the query
    /// Higher score = better match
    fn score_match(path: &Path, query: &str) -> u32 {
        let query_lower = query.to_lowercase();
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if filename == query_lower {
            // Exact filename match
            100
        } else if filename.starts_with(&query_lower) {
            // Filename starts with query
            80
        } else if filename.contains(&query_lower) {
            // Filename contains query
            60
        } else {
            // Path contains query (already filtered)
            40
        }
    }

    /// Search for files matching the query
    fn search_files(root: &Path, query: &str, max_results: usize) -> Vec<String> {
        let query_lower = query.to_lowercase();
        let mut matches: Vec<(String, u32)> = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories, only match files
            if !path.is_file() {
                continue;
            }

            // Check if path contains the query (case-insensitive)
            let path_str = path.to_string_lossy().to_lowercase();
            if path_str.contains(&query_lower) {
                let score = Self::score_match(path, &query_lower);
                matches.push((path.to_string_lossy().to_string(), score));
            }
        }

        // Sort by score descending, then by path length ascending (shorter paths first)
        matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.len().cmp(&b.0.len())));

        matches
            .into_iter()
            .take(max_results)
            .map(|(path, _)| path)
            .collect()
    }

    fn parse_input_json(raw: &str) -> Result<FindFilesInput, Error> {
        let parsed: FindFilesInputJson =
            serde_json::from_str(raw).map_err(|e| Error::Parse(e.to_string()))?;
        if parsed.query.is_empty() {
            return Err(Error::Parse("query is required".into()));
        }

        Ok(FindFilesInput {
            raw: raw.to_string(),
            query: parsed.query,
            root: parsed.root,
            max_results: parsed.max_results.unwrap_or(20),
        })
    }

    fn load_input(&self) -> Result<FindFilesInput, Error> {
        self.input
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Parse("input not parsed".into()))
    }
}

impl Tool for FindFiles {
    fn name(&self) -> &'static str {
        "find_files"
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

    fn work(&self, request: &dyn Request) -> ToolResult {
        let input = match self.load_input() {
            Ok(input) => input,
            Err(e) => {
                return ToolResult::error(self.name().to_string(), String::new(), e.to_string())
            }
        };

        // Resolve and validate search root
        let search_root =
            match Self::resolve_search_root(input.root.as_deref(), request.project_root()) {
                Ok(root) => root,
                Err(e) => return ToolResult::error(self.name().to_string(), input.raw, e),
            };

        // Check if search root exists
        if !search_root.exists() {
            return ToolResult::error(
                self.name().to_string(),
                input.raw,
                format!("Search root does not exist: {}", search_root.display()),
            );
        }

        // Search for files
        let results = Self::search_files(&search_root, &input.query, input.max_results);

        // Format output
        let output = if results.is_empty() {
            format!("No files found matching '{}'", input.query)
        } else {
            let mut output = format!(
                "Found {} file(s) matching '{}':\n",
                results.len(),
                input.query
            );
            for path in &results {
                output.push_str(&format!("  - {}\n", path));
            }
            output
        };

        ToolResult::ok(self.name().to_string(), input.raw, output)
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "substring to match against path/filename"
                },
                "root": {
                    "type": "string",
                    "description": "optional; default is project root"
                },
                "max_results": {
                    "type": "number",
                    "description": "optional; default 20"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use the `{}` tool to find files by substring match.",
            self.name()
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
            Ok(input) => {
                let label = short_label_from_path(&input.query);
                if label.is_empty() {
                    "Finding files".to_string()
                } else {
                    format!("Finding {}", label)
                }
            }
            Err(_) => "Finding files".to_string(),
        }
    }

    fn get_affected_paths(&self, request: &dyn Request) -> Vec<PathBuf> {
        match self.input.lock().unwrap().as_ref() {
            Some(input) => {
                let root_path = match input.root.as_deref() {
                    Some(root) => {
                        let path = PathBuf::from(root);
                        if path.is_absolute() {
                            path
                        } else {
                            request.project_root().join(path)
                        }
                    }
                    None => request.project_root().to_path_buf(),
                };
                vec![root_path]
            }
            None => vec![],
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Default for FindFiles {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_resolve_search_root_with_project_root() {
        // When no input root and project root is set, use project root
        let result = FindFiles::resolve_search_root(None, Path::new("/tmp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/tmp"));
    }

    #[test]
    fn test_resolve_search_root_with_input() {
        // When input root is provided, validate it's within project root
        let tmp_dir = tempfile::tempdir().unwrap();
        let subdir = tmp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();

        let result = FindFiles::resolve_search_root(subdir.to_str(), tmp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), subdir);
    }

    #[test]
    fn test_score_match_exact() {
        assert_eq!(
            FindFiles::score_match(Path::new("/foo/cat.py"), "cat.py"),
            100
        );
    }

    #[test]
    fn test_score_match_starts_with() {
        assert_eq!(
            FindFiles::score_match(Path::new("/foo/cat_utils.py"), "cat"),
            80
        );
    }

    #[test]
    fn test_score_match_contains() {
        assert_eq!(
            FindFiles::score_match(Path::new("/foo/test_cat.py"), "cat"),
            60
        );
    }

    #[test]
    fn test_score_match_path_only() {
        assert_eq!(FindFiles::score_match(Path::new("/cat/foo.py"), "cat"), 40);
    }

    #[test]
    fn test_search_prioritizes_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test_cat.py"), "").unwrap();
        std::fs::write(dir.path().join("cat.py"), "").unwrap();
        std::fs::write(dir.path().join("cat_utils.py"), "").unwrap();

        let results = FindFiles::search_files(dir.path(), "cat.py", 3);

        // Exact match should be first
        assert!(results[0].ends_with("cat.py"));
        assert!(!results[0].contains("test_"));
    }
}
