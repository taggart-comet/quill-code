use crate::domain::session::Request;
use crate::domain::tools::{Error, Tool, ToolResult};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Write};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const DELAY_SECONDS: u64 = 5;

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
}

impl Tool for ShellExec {
    fn name(&self) -> &'static str {
        "shell_exec"
    }

    fn parse_input(&self, input: String) -> Option<Error> {
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
                        input.raw,
                        format!("Working directory does not exist: {}", dir),
                    );
                }
                if !crate::utils::paths::is_within_root(path, request.project_root()) {
                    return ToolResult::error(
                        self.name().to_string(),
                        input.raw,
                        "Working directory is outside project root".to_string(),
                    );
                }
                path.to_path_buf()
            }
            None => request.project_root().to_path_buf(),
        };

        // Warn user and give time to cancel
        Self::warn_and_wait(&input.command, &work_dir);

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
                    input.raw,
                    format!("Failed to execute command: {}", e),
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
            return ToolResult::error(self.name().to_string(), input.raw, result);
        };

        ToolResult::ok(self.name().to_string(), input.raw, result)
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
            r#"Use the `{}` tool to execute shell commands.
Please DO NOT use it to read the full content of a file, this is not efficient, use `read_objects` tool for this."#,
            self.name()
        )
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

impl ShellExec {
    /// Display warning and countdown before executing command
    #[cfg(not(test))]
    fn warn_and_wait(command: &str, work_dir: &std::path::Path) {
        println!("\n\x1b[33m┌─ Action Approval: shell_exec ─────────────────────────────\x1b[0m");
        println!("\x1b[33m│\x1b[0m Command: \x1b[1m{}\x1b[0m", command);
        println!("\x1b[33m│\x1b[0m Workdir: {}", work_dir.display());
        println!("\x1b[33m│\x1b[0m");
        print!("\x1b[33m│\x1b[0m Executing in: ");
        let _ = io::stdout().flush();

        for i in (1..=DELAY_SECONDS).rev() {
            print!("\x1b[1m{}\x1b[0m ", i);
            let _ = io::stdout().flush();
            thread::sleep(Duration::from_secs(1));
        }

        println!("\n\x1b[33m│\x1b[0m \x1b[32mExecuting...\x1b[0m");
        println!("\x1b[33m└────────────────────────────────────────────────\x1b[0m\n");
    }

    #[cfg(test)]
    fn warn_and_wait(_command: &str, _work_dir: &std::path::Path) {
        // Skip delay in tests
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::VirtualRequest;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_shell_exec_echo() {
        let temp = tempdir().unwrap();
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"echo hello"}"#.to_string())
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
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"pwd"}"#.to_string())
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

        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(format!(
                r#"{{"command":"pwd","working_dir":"{}"}}"#,
                subdir.display()
            ))
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
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"pwd","working_dir":"/tmp"}"#.to_string())
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
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"exit 1"}"#.to_string())
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
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"echo error >&2 && exit 1"}"#.to_string())
            .is_none());
        let result = tool.work(&request);

        assert!(result.output_string().contains("Error"));
        assert!(result.output_string().contains("error"));
    }

    #[test]
    fn test_shell_exec_empty_command() {
        let temp = tempdir().unwrap();
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        let err = tool.parse_input(r#"{"command":""}"#.to_string());
        assert!(err.is_some());
        let result = tool.work(&request);
        assert!(result.output_string().contains("Error"));
    }

    #[test]
    fn test_shell_exec_creates_file() {
        let temp = tempdir().unwrap();
        let request = VirtualRequest::new("test", temp.path());
        let file_path = temp.path().join("created.txt");

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(format!(
                r#"{{"command":"echo 'test content' > {}"}}"#,
                file_path.display()
            ))
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
        let request = VirtualRequest::new("test", temp.path());

        let tool = ShellExec::new();
        assert!(tool
            .parse_input(r#"{"command":"echo 'hello world' | tr 'a-z' 'A-Z'"}"#.to_string())
            .is_none());
        let result = tool.work(&request);

        assert!(
            result.output_string().contains("HELLO WORLD"),
            "Expected uppercase, got: {}",
            result.output_string()
        );
    }
}
