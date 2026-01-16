use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolInput, ToolResult};
use crate::utils::paths::is_within_root;
use std::path::{Path, PathBuf};
use termtree::Tree;

pub struct Structure;

impl Structure {
    /// Resolve the target path, defaulting to project_root if "." or empty
    fn resolve_path(input_path: &str, project_root: &Path) -> Result<PathBuf, String> {
        let path = if input_path.is_empty() || input_path == "." {
            project_root.to_path_buf()
        } else {
            let input = Path::new(input_path);
            if input.is_absolute() {
                input.to_path_buf()
            } else {
                project_root.join(input)
            }
        };

        if !is_within_root(&path, project_root) {
            return Err(format!(
                "Path '{}' is outside project root '{}'",
                input_path,
                project_root.display()
            ));
        }

        Ok(path)
    }

    /// Build directory tree using termtree
    fn build_tree(root: &Path, max_depth: usize) -> String {
        let tree = Self::build_tree_recursive(root, 0, max_depth, true);
        tree.to_string()
    }

    fn build_tree_recursive(
        path: &Path,
        depth: usize,
        max_depth: usize,
        is_root: bool,
    ) -> Tree<String> {
        let name = if is_root {
            "./".to_string()
        } else {
            let base = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if path.is_dir() {
                format!("{}/", base)
            } else {
                base
            }
        };

        let mut tree = Tree::new(name);

        if path.is_dir() && depth < max_depth {
            let mut entries: Vec<_> = match std::fs::read_dir(path) {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        !name.starts_with('.')
                            && name != "node_modules"
                            && name != "target"
                            && name != "__pycache__"
                            && name != "venv"
                    })
                    .collect(),
                Err(_) => return tree,
            };

            // Sort: directories first, then files, both alphabetically
            entries.sort_by(|a, b| {
                let a_is_dir = a.path().is_dir();
                let b_is_dir = b.path().is_dir();
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.file_name().cmp(&b.file_name()),
                }
            });

            for entry in entries {
                let child = Self::build_tree_recursive(&entry.path(), depth + 1, max_depth, false);
                tree.push(child);
            }
        }

        tree
    }
}

impl Tool for Structure {
    fn name(&self) -> &'static str {
        "structure"
    }

    fn work(&self, input: &ToolInput, request: &dyn Request) -> ToolResult {
        let path = input.get_text("path").unwrap_or_else(|| ".".to_string());
        let max_depth = input.get_int("max_depth").map(|n| n as usize).unwrap_or(3);

        let target_path = match Self::resolve_path(&path, request.project_root()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(self.name(), input, e),
        };

        if !target_path.exists() {
            return ToolResult::error(
                self.name(),
                input,
                format!("Path does not exist: {}", target_path.display()),
            );
        }

        if !target_path.is_dir() {
            return ToolResult::error(
                self.name(),
                input,
                format!("Path is not a directory: {}", target_path.display()),
            );
        }

        let tree = Self::build_tree(&target_path, max_depth);

        ToolResult::ok(self.name(), input, tree)
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{}` tool to get directory structure. Fill the input format precisely:

<tool_name>{}</tool_name>
<input>
  <path>string</path>       # directory path to explore (use "." for project root)
  <max_depth>integer</max_depth> # optional; how deep to traverse (default: 3)
</input>"#,
            self.name(),
            self.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_path_dot() {
        let temp = tempdir().unwrap();
        let project_root = temp.path();
        let result = Structure::resolve_path(".", project_root);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), project_root.to_path_buf());
    }

    #[test]
    fn test_resolve_path_empty() {
        let temp = tempdir().unwrap();
        let project_root = temp.path();
        let result = Structure::resolve_path("", project_root);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), project_root.to_path_buf());
    }

    #[test]
    fn test_resolve_path_relative() {
        let temp = tempdir().unwrap();
        let project_root = temp.path();
        std::fs::create_dir(project_root.join("src")).unwrap();
        let result = Structure::resolve_path("src", project_root);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), project_root.join("src"));
    }

    #[test]
    fn test_resolve_path_outside_project() {
        let temp = tempdir().unwrap();
        let project_root = temp.path();
        let result = Structure::resolve_path("/etc", project_root);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside project root"));
    }

    #[test]
    fn test_build_tree() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::create_dir(root.join("tests")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        let tree = Structure::build_tree(root, 2);

        // Should start with ./
        assert!(tree.starts_with("./"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("tests/"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("lib.rs"));
    }
}
