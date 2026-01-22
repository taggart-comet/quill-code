use crate::domain::session::Request;
use crate::domain::tools::{Error, FileChange, Tool, ToolResult};
use serde::Deserialize;
use serde_json::json;
use similar::TextDiff;
use std::fs;
use std::path::{Component, PathBuf};
use std::sync::Mutex;
use zenpatch::apply as apply_patch;
use zenpatch::parser::text_to_patch::text_to_patch;
use zenpatch::Vfs;

pub struct PatchFiles {
    input: Mutex<Option<PatchFilesInput>>,
}

#[derive(Debug, Clone)]
struct PatchFilesInput {
    raw: String,
    patch: String,
}

#[derive(Debug, Deserialize)]
struct PatchFilesInputJson {
    patch: String,
}

impl PatchFiles {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }

    fn parse_input_json(raw: &str) -> Result<PatchFilesInput, Error> {
        let parsed: PatchFilesInputJson =
            serde_json::from_str(raw).map_err(|e| Error::Parse(e.to_string()))?;
        if parsed.patch.trim().is_empty() {
            return Err(Error::Parse("patch is required".into()));
        }
        Ok(PatchFilesInput {
            raw: raw.to_string(),
            patch: parsed.patch,
        })
    }

    fn load_input(&self) -> Result<PatchFilesInput, Error> {
        self.input
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Parse("input not parsed".into()))
    }
}

impl Tool for PatchFiles {
    fn name(&self) -> &'static str {
        "patch_files"
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

        let actions = match text_to_patch(&input.patch) {
            Ok(actions) => actions,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw,
                    format!("Failed to parse patch: {}", e),
                )
            }
        };

        let mut vfs = Vfs::new();
        let mut touched_paths: Vec<String> = Vec::new();
        let mut original_contents: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for action in &actions {
            if !touched_paths.contains(&action.path) {
                touched_paths.push(action.path.clone());
            }
            if let Some(new_path) = &action.new_path {
                if !touched_paths.contains(new_path) {
                    touched_paths.push(new_path.clone());
                }
            }
        }

        let project_root = request.project_root();
        for path in &touched_paths {
            let rel_path = std::path::Path::new(path);
            if rel_path.is_absolute()
                || rel_path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw,
                    format!("Invalid path outside project root: {}", path),
                );
            }
            let fs_path = project_root.join(rel_path);
            if fs_path.exists() {
                match fs::read_to_string(&fs_path) {
                    Ok(content) => {
                        original_contents.insert(path.clone(), content.clone());
                        vfs.insert(path.clone(), content);
                    }
                    Err(e) => {
                        return ToolResult::error(
                            self.name().to_string(),
                            input.raw,
                            format!("Failed to read file '{}': {}", fs_path.display(), e),
                        )
                    }
                }
            }
        }

        let new_vfs = match apply_patch(&input.patch, &vfs) {
            Ok(new_vfs) => new_vfs,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw,
                    format!("Failed to apply patch: {}", e),
                )
            }
        };

        for path in &touched_paths {
            let fs_path = project_root.join(path);
            if let Some(content) = new_vfs.get(path) {
                if let Some(parent) = fs_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        return ToolResult::error(
                            self.name().to_string(),
                            input.raw,
                            format!(
                                "Failed to create parent directories for '{}': {}",
                                fs_path.display(),
                                e
                            ),
                        );
                    }
                }
                if let Err(e) = fs::write(&fs_path, content) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw,
                        format!("Failed to write file '{}': {}", fs_path.display(), e),
                    );
                }
            } else if fs_path.exists() {
                if let Err(e) = fs::remove_file(&fs_path) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw,
                        format!("Failed to delete file '{}': {}", fs_path.display(), e),
                    );
                }
            }
        }

        // Compute file changes
        let mut file_changes = Vec::new();
        for path in &touched_paths {
            let original = original_contents.get(path);
            let new_content = new_vfs.get(path);

            let (added_lines, deleted_lines) =
                if let (Some(orig), Some(new)) = (original, new_content) {
                    if orig != new {
                        compute_line_diff(orig, new)
                    } else {
                        (0, 0) // No change
                    }
                } else if original.is_some() && new_content.is_none() {
                    // Deleted: count original lines as deleted
                    (0, original.unwrap().lines().count() as u32)
                } else if original.is_none() && new_content.is_some() {
                    // Added: count new lines as added
                    (new_content.unwrap().lines().count() as u32, 0)
                } else {
                    (0, 0) // No content change
                };

            if added_lines > 0 || deleted_lines > 0 {
                file_changes.push(FileChange {
                    path: path.clone(),
                    added_lines,
                    deleted_lines,
                });
            }
        }

        let mut result = ToolResult::ok(
            self.name().to_string(),
            input.raw,
            "Patch applied successfully".to_string(),
        );

        if !file_changes.is_empty() {
            result = result.with_file_changes(file_changes);
        }

        result
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Begin Patch / Update File patch content; can include multiple file updates. \
        Example: \
        *** Begin Patch\\n\
        *** Update File: foo.php\\n\
        @@\\n\
        -old\\n\
        +new\\n\
        *** Update File: bar.md\\n\
        @@\\n\
        -Old title\\n\
        +New title\\n\
        *** End Patch"
                }
            },
            "required": ["patch"]
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use the `{name}` tool to edit one or more files using the Begin Patch / Update File patch format.",
            name = self.name()
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

    fn get_command(&self, _request: &dyn Request) -> Option<String> {
        None // Patch files doesn't execute commands
    }

    fn get_affected_paths(&self, request: &dyn Request) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(input) = self.load_input() {
            // Parse the patch to extract affected paths
            if let Ok(actions) = text_to_patch(&input.patch) {
                for action in &actions {
                    paths.push(request.project_root().join(&action.path));
                    if let Some(ref new_path) = action.new_path {
                        paths.push(request.project_root().join(new_path));
                    }
                }
            }
        }

        paths
    }
}

fn compute_line_diff(old: &str, new: &str) -> (u32, u32) {
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0;
    let mut deleted = 0;
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => deleted += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    (added, deleted)
}

impl Default for PatchFiles {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{Request, SessionRequest};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    struct TestRequest {
        root: PathBuf,
        current_request: String,
        history: Vec<SessionRequest>,
        final_message: Option<String>,
    }

    impl TestRequest {
        fn new(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                current_request: "test".to_string(),
                history: Vec::new(),
                final_message: None,
            }
        }
    }

    impl Request for TestRequest {
        fn history(&self) -> &[SessionRequest] {
            &self.history
        }

        fn current_request(&self) -> &str {
            &self.current_request
        }

        fn project_root(&self) -> &Path {
            &self.root
        }

        fn user_settings(&self) -> Option<&crate::domain::UserSettings> {
            None
        }

        fn project_id(&self) -> Option<i32> {
            None
        }

        fn set_final_message(&mut self, message: String) {
            self.final_message = Some(message);
        }
    }

    #[test]
    fn test_patch_file_simple_change() {
        let temp = tempdir().unwrap();

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let request = TestRequest::new(temp.path());

        let tool = PatchFiles::new();
        let patch = "*** Begin Patch\n\
*** Update File: test.txt\n\
@@\n\
-line2\n\
+line2_modified\n\
*** End Patch";
        let input = format!(
            "{{\"patch\":\"{}\"}}",
            patch.replace('\n', "\\n").replace('"', "\\\"")
        );
        assert!(tool.parse_input(input).is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("successfully"),
            "Expected success, got: {}",
            result.output_string()
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(
            content.contains("line2_modified"),
            "File content: {}",
            content
        );
    }

    #[test]
    fn test_patch_file_add_lines() {
        let temp = tempdir().unwrap();

        let file_path = temp.path().join("code.py");
        fs::write(&file_path, "def foo():\n    pass\n").unwrap();

        let request = TestRequest::new(temp.path());

        let tool = PatchFiles::new();
        let patch = "*** Begin Patch\n\
*** Update File: code.py\n\
@@\n\
 def foo():\n\
     pass\n\
+\n\
+def bar():\n\
+    pass\n\
*** End Patch";
        let input = format!(
            "{{\"patch\":\"{}\"}}",
            patch.replace('\n', "\\n").replace('"', "\\\"")
        );
        assert!(tool.parse_input(input).is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("successfully"),
            "Expected success, got: {}",
            result.output_string()
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("def bar():"), "File content: {}", content);
    }

    #[test]
    fn test_patch_file_empty_patch() {
        let temp = tempdir().unwrap();

        let request = TestRequest::new(temp.path());

        let tool = PatchFiles::new();
        let input = "{\"patch\":\"not a patch\"}".to_string();
        assert!(tool.parse_input(input).is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("Error"),
            "Expected error, got: {}",
            result.output_string()
        );
    }
}
