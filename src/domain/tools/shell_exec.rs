use crate::domain::session::Request;
use crate::domain::tools::{short_words, Error, Tool, ToolResult, TOOL_OUTPUT_BUDGET_CHARS};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

pub struct ShellExec {
    input: Mutex<Option<ShellExecInputParsed>>,
}

/// Input struct for the shell_exec tool - can be deserialized from XML
#[derive(Debug, Deserialize)]
#[serde(rename = "input")]
pub struct ShellExecInput {
    #[serde(rename = "command")]
    pub command: String,
    #[serde(rename = "working_dir", default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone)]
struct ShellExecInputParsed {
    raw: String,
    command: String,
    working_dir: Option<String>,
    call_id: String,
}

fn is_allowed_read_only_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.contains(';')
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('|')
        || trimmed.contains('>')
        || trimmed.contains('<')
    {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or_default();
    let args: Vec<&str> = parts.collect();

    match first {
        "rg" | "grep" | "glob" | "cat" | "head" | "tail" | "less" | "more" | "wc" | "cut"
        | "sort" | "uniq" | "find" | "ls" | "tree" | "stat" | "file" | "awk" | "pwd"
        | "which" | "type" => true,
        "sed" => args.iter().any(|arg| *arg == "-n" || *arg == "--quiet" || *arg == "--silent"),
        _ => false,
    }
}

impl Tool for ShellExec {
    fn name(&self) -> &'static str {
        "shell_exec"
    }

    fn parse_input(&self, input: String, call_id: String) -> Option<Error> {
        let trimmed = input.trim();
        let parsed = serde_json::from_str::<ShellExecInput>(trimmed)
            .map_err(|e| Error::Parse(e.to_string()));

        match parsed {
            Ok(parsed) => {
                if parsed.command.trim().is_empty() {
                    return Some(Error::Parse("command cannot be empty".into()));
                }
                *self.input.lock().unwrap() = Some(ShellExecInputParsed {
                    raw: trimmed.to_string(),
                    command: parsed.command,
                    working_dir: parsed.working_dir,
                    call_id,
                });
                None
            }
            Err(err) => Some(err),
        }
    }

    fn work(&self, request: &dyn Request) -> ToolResult {
        let input = match self.input.lock().unwrap().clone() {
            Some(input) => input,
            None => {
                return ToolResult::error(
                    self.name().to_string(),
                    String::new(),
                    "input not parsed".to_string(),
                    String::new(),
                )
            }
        };

        // Determine working directory
        let work_dir = match &input.working_dir {
            Some(dir) => {
                let path = std::path::Path::new(dir);
                if !path.exists() {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        format!("Working directory does not exist: {}", dir),
                        input.call_id.clone(),
                    );
                }
                if !crate::utils::paths::is_within_root(path, request.project_root()) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        "Working directory is outside project root".to_string(),
                        input.call_id.clone(),
                    );
                }
                path.to_path_buf()
            }
            None => request.project_root().to_path_buf(),
        };

        // Execute the command
        let output = match Command::new("bash")
            .arg("-c")
            .arg(&input.command)
            .current_dir(&work_dir)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    format!("Failed to execute command: {}", e),
                    input.call_id.clone(),
                )
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let result = if output.status.success() {
            if stdout.is_empty() && stderr.is_empty() {
                "Command executed successfully (no output)".to_string()
            } else if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n[stderr]: {}", stdout, stderr)
            }
        } else {
            let code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or("unknown".to_string());
            let mut result = format!("Command failed with exit code {}\n", code);
            if !stdout.is_empty() {
                result.push_str(&format!("[stdout]: {}\n", stdout));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("[stderr]: {}", stderr));
            }
            return ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                result,
                input.call_id.clone(),
            );
        };

        ToolResult::ok(self.name().to_string(), input.raw, result, input.call_id)
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "the command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "optional; directory to run command in (default: project root)"
                }
            },
            "required": ["command"],
            "additionalProperties": false
        })
    }

    fn desc(&self) -> String {
        format!(
            "Use `{}` tool to execute shell commands.
DO NOT use it to read code, this is not efficient, use `read_objects` tool for this. \n\
DO NOT use it to change files, use `patch_files` tool for this.",
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
        let command = self
            .input
            .lock()
            .unwrap()
            .as_ref()
            .map(|input| input.command.clone())
            .unwrap_or_default();
        let label = short_words(&command, 2);
        if label.is_empty() {
            "Running command".to_string()
        } else {
            format!("Running {}", label)
        }
    }

    fn get_command(&self, _request: &dyn Request) -> Option<String> {
        match self.input.lock().unwrap().as_ref() {
            Some(input) => Some(input.command.clone()),
            None => None,
        }
    }

    fn get_affected_paths(&self, request: &dyn Request) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(input) = self.input.lock().unwrap().as_ref() {
            // Add working directory if specified
            if let Some(ref working_dir) = input.working_dir {
                paths.push(PathBuf::from(working_dir));
            }

            // Try to extract file paths from common commands
            let command = &input.command;

            // Extract paths from commands like `rm file.txt`, `touch file.txt`, etc.
            if command.starts_with("rm ")
                || command.starts_with("touch ")
                || command.starts_with("mkdir ")
            {
                let parts: Vec<&str> = command.split_whitespace().collect();
                for part in parts.iter().skip(1) {
                    if !part.starts_with('-') {
                        // Skip flags
                        let path = PathBuf::from(part);
                        if !path.is_absolute() {
                            paths.push(request.project_root().join(path));
                        } else {
                            paths.push(path);
                        }
                    }
                }
            }

            // Extract paths from redirect operations like `echo content > file.txt`
            if let Some(redirect_pos) = command.find('>') {
                let after_redirect = &command[redirect_pos + 1..].trim();
                if let Some(file_path) = after_redirect.split_whitespace().next() {
                    let path = PathBuf::from(file_path);
                    if !path.is_absolute() {
                        paths.push(request.project_root().join(path));
                    } else {
                        paths.push(path);
                    }
                }
            }
        }

        paths
    }

    fn is_read_only(&self) -> bool {
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .map(|input| is_allowed_read_only_command(&input.command))
            .unwrap_or(false)
    }
}

impl ShellExec {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }
}

impl Default for ShellExec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{Request, SessionRequest};
    use std::fs;
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
    fn test_shell_exec_echo() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"echo hello"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("hello"),
            "Expected 'hello', got: {}",
            result.output_string()
        );
    }

    #[test]
    fn test_shell_exec_pwd() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"pwd"}"#.to_string(), "call-id".to_string())
            .is_none());
        let result = tool.work(&request);

        // Should contain the temp directory path
        let temp_path = temp.path().canonicalize().unwrap();
        assert!(
            result.output_string().contains(temp_path.to_str().unwrap()),
            "Expected path '{}', got: {}",
            temp_path.display(),
            result.output_string()
        );
    }

    #[test]
    fn test_shell_exec_working_dir() {
        let temp = tempdir().unwrap();
        let subdir = temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                format!(
                    r#"{{"command":"pwd","working_dir":"{}"}}"#,
                    subdir.display()
                ),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        let subdir_canonical = subdir.canonicalize().unwrap();
        assert!(
            result
                .output_string()
                .contains(subdir_canonical.to_str().unwrap()),
            "Expected subdir path, got: {}",
            result.output_string()
        );
    }

    #[test]
    fn test_shell_exec_working_dir_outside_project() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"pwd","working_dir":"/tmp"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("Error"),
            "Expected error, got: {}",
            result.output_string()
        );
        assert!(result.output_string().contains("outside project root"));
    }

    #[test]
    fn test_shell_exec_failed_command() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"exit 1"}"#.to_string(), "call-id".to_string())
            .is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("Error"),
            "Expected error, got: {}",
            result.output_string()
        );
        assert!(result.output_string().contains("exit code 1"));
    }

    #[test]
    fn test_shell_exec_command_with_stderr() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"echo error >&2 && exit 1"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        assert!(result.output_string().contains("Error"));
        assert!(result.output_string().contains("error"));
    }

    #[test]
    fn test_shell_exec_empty_command() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        let err = tool.parse_input(r#"{"command":""}"#.to_string(), "call-id".to_string());
        assert!(err.is_some());
        let result = tool.work(&request);
        assert!(result.output_string().contains("Error"));
    }

    #[test]
    fn test_shell_exec_creates_file() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());
        let file_path = temp.path().join("created.txt");

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                format!(
                    r#"{{"command":"echo 'test content' > {}"}}"#,
                    file_path.display()
                ),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        assert!(
            !result.output_string().contains("Error"),
            "Unexpected error: {}",
            result.output_string()
        );
        assert!(file_path.exists(), "File should have been created");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("test content"));
    }

    #[test]
    fn test_shell_exec_piped_commands() {
        let temp = tempdir().unwrap();
        let request = TestRequest::new(temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"echo 'hello world' | tr 'a-z' 'A-Z'"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("HELLO WORLD"),
            "Expected uppercase, got: {}",
            result.output_string()
        );
    }

    #[test]
    fn test_shell_exec_is_read_only_allows_read_commands() {
        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"rg -n foo src"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(tool.is_read_only());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"grep -n foo file.txt"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(tool.is_read_only());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"sed -n '1,10p' file.txt"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_shell_exec_is_read_only_rejects_write_or_compound() {
        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"sed '1,10p' file.txt"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(!tool.is_read_only());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"echo hi > out.txt"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(!tool.is_read_only());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(
                r#"{"command":"echo hi && rg -n foo src"}"#.to_string(),
                "call-id".to_string()
            )
            .is_none());
        assert!(!tool.is_read_only());
    }
}