use crate::domain::session::Request;
use crate::domain::tools::{short_filename, Error, FileChange, Tool, ToolResult};
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
    call_id: String,
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
            call_id: String::new(),
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

    fn parse_input(&self, input: String, call_id: String) -> Option<Error> {
        let trimmed = input.trim();
        let parsed = Self::parse_input_json(trimmed);

        match parsed {
            Ok(mut parsed) => {
                parsed.call_id = call_id;
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
                return ToolResult::error(
                    self.name().to_string(),
                    String::new(),
                    e.to_string(),
                    String::new(),
                )
            }
        };

        let actions = match text_to_patch(&input.patch) {
            Ok(actions) => actions,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    format!("Failed to parse patch: {}", e),
                    input.call_id.clone(),
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
                    input.raw.clone(),
                    format!("Invalid path outside project root: {}", path),
                    input.call_id.clone(),
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
                            input.raw.clone(),
                            format!("Failed to read file '{}': {}", fs_path.display(), e),
                            input.call_id.clone(),
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
                    input.raw.clone(),
                    format!("Patch Failed: {}. Next step: read the file region you’re editing (function/block + ~10 lines around it) and generate a new patch using those exact lines as context. Then retry patch_files.", e),
                    input.call_id.clone(),
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
                            input.raw.clone(),
                            format!(
                                "Failed to create parent directories for '{}': {}",
                                fs_path.display(),
                                e
                            ),
                            input.call_id.clone(),
                        );
                    }
                }
                if let Err(e) = fs::write(&fs_path, content) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        format!("Failed to write file '{}': {}", fs_path.display(), e),
                        input.call_id.clone(),
                    );
                }
            } else if fs_path.exists() {
                if let Err(e) = fs::remove_file(&fs_path) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        format!("Failed to delete file '{}': {}", fs_path.display(), e),
                        input.call_id.clone(),
                    );
                }
            }
        }

        // Compute file changes
        let mut file_changes = Vec::new();
        for path in &touched_paths {
            let original = original_contents.get(path).map(|s| s.as_str());
            let new_content = new_vfs.get(path).map(|s| s.as_str());

            if let Some(change) = compute_file_change(path, original, new_content) {
                file_changes.push(change);
            }
        }

        let mut result = ToolResult::ok(
            self.name().to_string(),
            input.raw,
            "Patch applied successfully".to_string(),
            input.call_id,
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

    fn get_output_budget(&self) -> Option<usize> {
        None
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
        let input = match self.load_input() {
            Ok(input) => input,
            Err(_) => return "Patching files".to_string(),
        };
        let actions = match text_to_patch(&input.patch) {
            Ok(actions) => actions,
            Err(_) => return "Patching files".to_string(),
        };
        let mut names = Vec::new();
        for action in &actions {
            let name = short_filename(&action.path);
            if !names.contains(&name) {
                names.push(name);
            }
            if let Some(new_path) = &action.new_path {
                let new_name = short_filename(new_path);
                if !names.contains(&new_name) {
                    names.push(new_name);
                }
            }
        }
        if names.is_empty() {
            "Patching files".to_string()
        } else {
            format!("Patching {}", names.join(", "))
        }
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

const DIFF_LINE_LIMIT: usize = 2000;

fn compute_file_change(path: &str, old: Option<&str>, new: Option<&str>) -> Option<FileChange> {
    let old_lines = old.map(|content| content.lines().count()).unwrap_or(0);
    let new_lines = new.map(|content| content.lines().count()).unwrap_or(0);
    let max_lines = old_lines.max(new_lines);
    let large_file = max_lines > DIFF_LINE_LIMIT;

    match (old, new) {
        (Some(old_content), Some(new_content)) => {
            if old_content == new_content {
                return None;
            }
            if large_file {
                return Some(FileChange {
                    path: path.to_string(),
                    added_lines: new_lines.saturating_sub(old_lines) as u32,
                    deleted_lines: old_lines.saturating_sub(new_lines) as u32,
                    unified_diff: format!("Diff omitted for large file ({} lines).", max_lines),
                });
            }

            let diff = TextDiff::from_lines(old_content, new_content);
            let mut added = 0;
            let mut deleted = 0;
            let mut result = String::new();

            result.push_str(&format!("--- a/{}\n", path));
            result.push_str(&format!("+++ b/{}\n", path));

            let mut old_line = 1;
            let mut new_line = 1;
            let mut hunk_changes = Vec::new();
            let mut hunk_old_start = 1;
            let mut hunk_new_start = 1;

            for change in diff.iter_all_changes() {
                if hunk_changes.is_empty() {
                    hunk_old_start = old_line;
                    hunk_new_start = new_line;
                }

                match change.tag() {
                    similar::ChangeTag::Equal => {
                        hunk_changes.push(format!(" {}", change));
                        old_line += 1;
                        new_line += 1;
                    }
                    similar::ChangeTag::Delete => {
                        hunk_changes.push(format!("-{}", change));
                        old_line += 1;
                        deleted += 1;
                    }
                    similar::ChangeTag::Insert => {
                        hunk_changes.push(format!("+{}", change));
                        new_line += 1;
                        added += 1;
                    }
                }
            }

            if !hunk_changes.is_empty() {
                let hunk_old_len = old_line - hunk_old_start;
                let hunk_new_len = new_line - hunk_new_start;
                result.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n",
                    hunk_old_start, hunk_old_len, hunk_new_start, hunk_new_len
                ));
                for change in hunk_changes {
                    result.push_str(&change);
                }
            }

            Some(FileChange {
                path: path.to_string(),
                added_lines: added,
                deleted_lines: deleted,
                unified_diff: result,
            })
        }
        (None, Some(new_content)) => {
            let added_lines = new_lines as u32;
            let unified_diff = if large_file {
                format!("Diff omitted for large file ({} lines).", max_lines)
            } else {
                let mut result = String::new();
                result.push_str("--- /dev/null\n");
                result.push_str(&format!("+++ b/{}\n", path));
                result.push_str(&format!("@@ -0,0 +1,{} @@\n", new_lines));
                for line in new_content.lines() {
                    result.push_str(&format!("+{}\n", line));
                }
                result
            };

            Some(FileChange {
                path: path.to_string(),
                added_lines,
                deleted_lines: 0,
                unified_diff,
            })
        }
        (Some(old_content), None) => {
            let deleted_lines = old_lines as u32;
            let unified_diff = if large_file {
                format!("Diff omitted for large file ({} lines).", max_lines)
            } else {
                let mut result = String::new();
                result.push_str(&format!("--- a/{}\n", path));
                result.push_str("+++ /dev/null\n");
                result.push_str(&format!("@@ -1,{} +0,0 @@\n", old_lines));
                for line in old_content.lines() {
                    result.push_str(&format!("-{}\n", line));
                }
                result
            };

            Some(FileChange {
                path: path.to_string(),
                added_lines: 0,
                deleted_lines,
                unified_diff,
            })
        }
        (None, None) => None,
    }
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
        final_message: Option<String>,
    }

    impl TestRequest {
        fn new(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                current_request: "test".to_string(),
                final_message: None,
            }
        }
    }

    impl Request for TestRequest {
        fn history(&self) -> &[SessionRequest] {
            &[]
        }

        fn current_request(&self) -> &str {
            &self.current_request
        }

        fn mode(&self) -> crate::domain::AgentModeType {
            crate::domain::AgentModeType::Build
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

        fn images(&self) -> &[String] {
            &[]
        }

        fn session_id(&self) -> Option<i64> {
            None
        }

        fn get_history_steps(&self) -> Vec<crate::domain::workflow::step::ChainStep> {
            Vec::new()
        }

        fn get_session_plan(&self) -> Option<crate::domain::todo::TodoList> {
            None
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
        assert!(tool.parse_input(input, "call-id".to_string()).is_none());
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
        assert!(tool.parse_input(input, "call-id".to_string()).is_none());
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
        assert!(tool.parse_input(input, "call-id".to_string()).is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("Error"),
            "Expected error, got: {}",
            result.output_string()
        );
    }
}
