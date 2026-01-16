use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolInput, ToolResult};
use crate::utils::paths::is_within_root;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct FindFiles;

impl FindFiles {
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
}

impl Tool for FindFiles {
    fn name(&self) -> &'static str {
        "find_files"
    }

    fn work(&self, input: &ToolInput, request: &dyn Request) -> ToolResult {
        // Parse query
        let query = match input.require_text("query") {
            Ok(q) => q,
            Err(e) => return ToolResult::error(self.name(), input, e),
        };

        if query.is_empty() {
            return ToolResult::error(self.name(), input, "Query is required".to_string());
        }

        // Parse optional fields
        let root = input.get_text("root");
        let max_results = input
            .get_int("max_results")
            .map(|n| n as usize)
            .unwrap_or(20);

        // Resolve and validate search root
        let search_root = match Self::resolve_search_root(root.as_deref(), request.project_root()) {
            Ok(root) => root,
            Err(e) => return ToolResult::error(self.name(), input, e),
        };

        // Check if search root exists
        if !search_root.exists() {
            return ToolResult::error(
                self.name(),
                input,
                format!("Search root does not exist: {}", search_root.display()),
            );
        }

        // Search for files
        let results = Self::search_files(&search_root, &query, max_results);

        // Format output
        let output = if results.is_empty() {
            format!("No files found matching '{}'", query)
        } else {
            let mut output = format!("Found {} file(s) matching '{}':\n", results.len(), query);
            for path in &results {
                output.push_str(&format!("  - {}\n", path));
            }
            output
        };

        ToolResult::ok(self.name(), input, output)
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{}` tool to find files by substring match. Fill the input format precisely:

<tool_name>{}</tool_name>
<input>
  <query>string</query>         # substring to match against path/filename
  <root>string</root>          # optional; default is project root
  <max_results>integer</max_results>  # optional; default 20
</input>"#,
            self.name(),
            self.name()
        )
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
        let result = FindFiles::resolve_search_root(Some("/tmp/subdir"), Path::new("/tmp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/tmp/subdir"));
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
