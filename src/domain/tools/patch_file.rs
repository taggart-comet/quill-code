use crate::domain::session::Request;
use crate::domain::tools::{escape_xml, Tool, ToolInput, ToolResult};
use std::fs;
use std::process::Command;

pub struct PatchFile;

impl PatchFile {
    /// Get all text content from an element, including text from all child text nodes
    /// This is needed for multi-line content like patches
    fn get_element_text_content(input: &ToolInput, tag: &str) -> Option<String> {
        let doc = roxmltree::Document::parse(input.raw()).ok()?;
        let element = doc.descendants().find(|n| n.has_tag_name(tag))?;

        // Collect all text from this element and its children
        let mut text = String::new();
        for node in element.descendants() {
            if node.is_text() {
                if let Some(text_content) = node.text() {
                    text.push_str(text_content);
                }
            }
        }

        Some(text)
    }
}

impl Tool for PatchFile {
    fn name(&self) -> &'static str {
        "patch_file"
    }

    fn work(&self, input: &ToolInput, request: &dyn Request) -> ToolResult {
        // Parse input - file_path is kept for validation/documentation but patch contains the actual paths
        let _file_path = match input.require_text("file_path") {
            Ok(p) => p,
            Err(e) => return ToolResult::error(self.name(), input, e),
        };

        // Get patch content
        let patch = match Self::get_element_text_content(input, "patch") {
            Some(p) if !p.trim().is_empty() => p,
            _ => return ToolResult::error(self.name(), input, "Missing or empty <patch> element"),
        };

        // Create temp file for the patch
        let patch_file = match tempfile::NamedTempFile::new() {
            Ok(f) => f,
            Err(e) => {
                return ToolResult::error(
                    self.name(),
                    input,
                    format!("Failed to create temp file: {}", e),
                )
            }
        };

        if let Err(e) = fs::write(patch_file.path(), &patch) {
            return ToolResult::error(self.name(), input, format!("Failed to write patch: {}", e));
        }

        // Apply with git apply
        let output = match Command::new("git")
            .arg("apply")
            .arg("--unidiff-zero")
            .arg("--verbose")
            .arg(patch_file.path())
            .current_dir(request.project_root())
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                return ToolResult::error(
                    self.name(),
                    input,
                    format!("Failed to execute git apply: {}", e),
                )
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            let msg = if stdout.is_empty() && stderr.is_empty() {
                "Patch applied successfully".to_string()
            } else {
                format!("Patch applied successfully\n{}{}", stdout, stderr)
            };
            ToolResult::ok(
                self.name(),
                input,
                format!("<result>{}</result>", escape_xml(&msg)),
            )
        } else {
            let mut error_msg = format!("Failed to apply patch\nPatch content:\n{}", patch);
            if !stderr.is_empty() {
                error_msg.push_str(&format!("\n[stderr]: {}", stderr));
            }
            ToolResult::error(self.name(), input, error_msg)
        }
    }

    fn spec(&self) -> String {
        format!(
            r#"Use the `{name}` tool to edit files using git apply.

<tool_name>{name}</tool_name>
<input>
  <file_path>src/main.rs</file_path>
  <patch>
 --- a/foo.py
+++ b/foo.py
@@
 def bar(x):
     return x * 2
+
+def baz(x):
+    return x + 1   
  </patch>
</input>
"#,
            name = self.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::VirtualRequest;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_git_repo(path: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("Failed to init git repo");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("Failed to set git config");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .expect("Failed to set git config");
    }

    #[test]
    fn test_patch_file_simple_change() {
        let temp = tempdir().unwrap();
        init_git_repo(temp.path());

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let request = VirtualRequest::new("test", temp.path());

        let xml = r#"<input>
            <file_path>test.txt</file_path>
            <patch>
--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line1
-line2
+line2_modified
 line3
            </patch>
        </input>"#;

        let input = ToolInput::new(xml).unwrap();
        let result = PatchFile.work(&input, &request);

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
        init_git_repo(temp.path());

        let file_path = temp.path().join("code.py");
        fs::write(&file_path, "def foo():\n    pass\n").unwrap();

        Command::new("git")
            .args(["add", "code.py"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let request = VirtualRequest::new("test", temp.path());

        let xml = r#"<input>
            <file_path>code.py</file_path>
            <patch>
--- a/code.py
+++ b/code.py
@@ -1,2 +1,5 @@
 def foo():
     pass
+
+def bar():
+    pass
            </patch>
        </input>"#;

        let input = ToolInput::new(xml).unwrap();
        let result = PatchFile.work(&input, &request);

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
        init_git_repo(temp.path());

        let request = VirtualRequest::new("test", temp.path());

        let xml = r#"<input>
            <file_path>test.txt</file_path>
            <patch></patch>
        </input>"#;

        let input = ToolInput::new(xml).unwrap();
        let result = PatchFile.work(&input, &request);

        assert!(
            result.output_string().contains("Error"),
            "Expected error, got: {}",
            result.output_string()
        );
    }
}
