use crate::domain::session::Request;
use crate::domain::tools::{Error, Tool, ToolResult};
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Component;
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
                return ToolResult::error(
                    self.name().to_string(),
                    String::new(),
                    e.to_string(),
                )
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

        ToolResult::ok(
            self.name().to_string(),
            input.raw,
            "Patch applied successfully".to_string(),
        )
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

}

impl Default for PatchFiles {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::VirtualRequest;
    use tempfile::tempdir;

    #[test]
    fn test_patch_file_simple_change() {
        let temp = tempdir().unwrap();

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let request = VirtualRequest::new("test", temp.path());

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

        let request = VirtualRequest::new("test", temp.path());

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

        let request = VirtualRequest::new("test", temp.path());

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
