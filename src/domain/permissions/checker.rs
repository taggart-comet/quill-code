use super::store::{PermissionStore, StoreError};
use super::types::{PermissionConfig, PermissionDecision, PermissionRequest};
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use crate::utils::paths::is_within_root;
use crate::utils::AskError;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CheckerError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("permission check failed: {0}")]
    Failed(String),
}

pub struct PermissionChecker {
    store: Arc<dyn PermissionStore>,
    config: PermissionConfig,
    prompter: Arc<dyn PermissionPrompter>,
}

pub trait PermissionPrompter: Send + Sync {
    fn ask_permission(&self, request: &PermissionRequest) -> Result<PermissionDecision, AskError>;
}

impl PermissionChecker {
    pub fn new_with_prompter(
        store: Arc<dyn PermissionStore>,
        config: PermissionConfig,
        prompter: Arc<dyn PermissionPrompter>,
    ) -> Self {
        Self {
            store,
            config,
            prompter,
        }
    }

    /// Check permission for a tool execution
    pub fn check(
        &self,
        tool: &dyn Tool,
        request: &dyn Request,
        project_id: Option<i32>,
    ) -> Result<bool, CheckerError> {
        let permission_request = PermissionRequest::new(
            tool.name().to_string(),
            tool.get_command(request),
            tool.get_affected_paths(request),
            super::types::PermissionScope::Project,
            project_id,
        );
        let decision = self.resolve_permission(&permission_request)?;
        if decision == PermissionDecision::Ask
            && self.is_allowed_by_default(tool, request, &permission_request)
        {
            return Ok(true);
        }
        match decision {
            PermissionDecision::AlwaysAllow | PermissionDecision::AllowOnce => Ok(true),
            PermissionDecision::AlwaysDeny => Ok(false),
            PermissionDecision::Ask => self.prompt_and_store(&permission_request),
        }
    }

    /// Store a permission decision
    pub fn store_permission_decision(
        &self,
        request: &PermissionRequest,
        decision: PermissionDecision,
    ) -> Result<(), CheckerError> {
        // Don't store AllowOnce decisions
        if decision == PermissionDecision::AllowOnce {
            return Ok(());
        }

        let resource_pattern =
            if decision == PermissionDecision::AlwaysAllow && request.tool_name == "web_search" {
                None
            } else if request.paths.len() == 1 {
                Some(request.paths[0].to_string_lossy().to_string())
            } else {
                None
            };

        let permission = super::types::Permission::new(
            request.tool_name.clone(),
            request.command.clone(),
            resource_pattern,
            decision,
            request.scope.clone(),
            request.project_id,
        );

        self.store.create_permission(permission)?;
        Ok(())
    }

    /// Check if a command is dangerous
    pub fn is_dangerous_command(&self, command: &str) -> bool {
        self.config
            .dangerous_commands
            .iter()
            .any(|dangerous| command.contains(dangerous) || command.starts_with(dangerous))
    }

    /// Check if a path is restricted
    pub fn is_restricted_path(&self, path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy();
        self.config
            .restricted_paths
            .iter()
            .any(|restricted| path_str.starts_with(restricted) || path_str.contains(restricted))
    }

    fn check_default_rules(
        &self,
        request: &PermissionRequest,
    ) -> Result<PermissionDecision, CheckerError> {
        // Check for dangerous commands
        if let Some(command) = &request.command {
            if self.is_dangerous_command(command) {
                return Ok(PermissionDecision::Ask);
            }
        }

        // Check for restricted paths
        for path in &request.paths {
            if self.is_restricted_path(path) {
                return Ok(PermissionDecision::Ask);
            }
        }

        // Use default decision
        Ok(self.config.default_decision.clone())
    }

    fn is_allowed_by_default(
        &self,
        tool: &dyn Tool,
        request: &dyn Request,
        permission_request: &PermissionRequest,
    ) -> bool {
        // Allow safe tools that don't require permission
        if tool.skip_permission_check() {
            return true;
        }

        // Allow read-only tools if paths are within project root
        if !tool.is_read_only() {
            return false;
        }

        let project_root = request.project_root();
        if permission_request.paths.is_empty() {
            return false;
        }

        permission_request.paths.iter().all(|path| {
            let normalized = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_root.join(path)
            };
            is_within_root(&normalized, project_root)
        })
    }

    fn resolve_permission(
        &self,
        request: &PermissionRequest,
    ) -> Result<PermissionDecision, CheckerError> {
        // Check specific command permissions first (highest priority)
        if let Some(command) = &request.command {
            if let Some(permission) = self.store.find_command_permission(
                &request.tool_name,
                command,
                request.project_id,
            )? {
                return Ok(permission.decision);
            }
        }

        // Check path-based permissions
        for path in &request.paths {
            if let Some(permission) =
                self.store
                    .find_path_permission(&request.tool_name, path, request.project_id)?
            {
                return Ok(permission.decision);
            }
        }

        // Check tool-level permissions
        if let Some(permission) = self
            .store
            .find_tool_permission(&request.tool_name, request.project_id)?
        {
            return Ok(permission.decision);
        }

        // Apply default security rules
        self.check_default_rules(request)
    }

    fn prompt_and_store(&self, request: &PermissionRequest) -> Result<bool, CheckerError> {
        match self.prompter.ask_permission(request) {
            Ok(decision @ PermissionDecision::AlwaysAllow)
            | Ok(decision @ PermissionDecision::AllowOnce) => {
                self.store_permission_decision(request, decision)?;
                Ok(true)
            }
            Ok(PermissionDecision::AlwaysDeny) => {
                self.store_permission_decision(request, PermissionDecision::AlwaysDeny)?;
                Ok(false)
            }
            Ok(PermissionDecision::Ask) => Ok(false),
            Err(AskError::IoError) => {
                Err(CheckerError::Failed("Permission prompt failed".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::permissions::types::Permission;
    use crate::domain::permissions::types::{PermissionConfig, PermissionDecision};
    use crate::domain::permissions::{PermissionRequest, PermissionScope};
    use crate::domain::session::{Request, SessionRequest};
    use crate::domain::tools::{Error, Tool, ToolResult};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct TestStore {
        created: Mutex<Vec<Permission>>,
        tool_permission: Mutex<Option<Permission>>,
        command_permission: Mutex<Option<Permission>>,
        path_permission: Mutex<Option<Permission>>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                tool_permission: Mutex::new(None),
                command_permission: Mutex::new(None),
                path_permission: Mutex::new(None),
            }
        }

        fn with_permissions(
            tool_permission: Option<Permission>,
            command_permission: Option<Permission>,
            path_permission: Option<Permission>,
        ) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                tool_permission: Mutex::new(tool_permission),
                command_permission: Mutex::new(command_permission),
                path_permission: Mutex::new(path_permission),
            }
        }
    }

    impl PermissionStore for TestStore {
        fn create_permission(&self, permission: Permission) -> Result<Permission, StoreError> {
            self.created.lock().unwrap().push(permission.clone());
            Ok(permission)
        }

        fn find_tool_permission(
            &self,
            _tool: &str,
            _project_id: Option<i32>,
        ) -> Result<Option<Permission>, StoreError> {
            let permission = self.tool_permission.lock().unwrap().clone();
            if let Some(permission) = permission {
                if permission.matches(_tool, None, None::<&PathBuf>) {
                    return Ok(Some(permission));
                }
            }
            Ok(None)
        }

        fn find_command_permission(
            &self,
            _tool: &str,
            _command: &str,
            _project_id: Option<i32>,
        ) -> Result<Option<Permission>, StoreError> {
            let permission = self.command_permission.lock().unwrap().clone();
            if let Some(permission) = permission {
                if permission.matches(_tool, Some(_command), None::<&PathBuf>) {
                    return Ok(Some(permission));
                }
            }
            Ok(None)
        }

        fn find_path_permission(
            &self,
            _tool: &str,
            _path: &PathBuf,
            _project_id: Option<i32>,
        ) -> Result<Option<Permission>, StoreError> {
            let permission = self.path_permission.lock().unwrap().clone();
            if let Some(permission) = permission {
                if permission.matches(_tool, None, Some(_path)) {
                    return Ok(Some(permission));
                }
            }
            Ok(None)
        }
    }

    struct TestPrompter {
        calls: Arc<AtomicUsize>,
        decision: PermissionDecision,
    }

    impl PermissionPrompter for TestPrompter {
        fn ask_permission(
            &self,
            _request: &PermissionRequest,
        ) -> Result<PermissionDecision, AskError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.decision.clone())
        }
    }

    struct TestRequest {
        root: PathBuf,
    }

    impl Request for TestRequest {
        fn history(&self) -> &[SessionRequest] {
            &[]
        }

        fn current_request(&self) -> &str {
            "test"
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

        fn set_final_message(&mut self, _message: String) {}

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

    struct ReadOnlyTool {
        paths: Vec<PathBuf>,
    }

    impl Tool for ReadOnlyTool {
        fn name(&self) -> &'static str {
            "read_only"
        }

        fn parse_input(&self, _input: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok("read_only".to_string(), String::new(), String::new())
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn desc(&self) -> String {
            "read only".to_string()
        }

        fn get_input(&self) -> String {
            String::new()
        }

        fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
            self.paths.clone()
        }

        fn is_read_only(&self) -> bool {
            true
        }
    }

    struct WriteTool {
        paths: Vec<PathBuf>,
    }

    impl Tool for WriteTool {
        fn name(&self) -> &'static str {
            "write_tool"
        }

        fn parse_input(&self, _input: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok("write_tool".to_string(), String::new(), String::new())
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn desc(&self) -> String {
            "write tool".to_string()
        }

        fn get_input(&self) -> String {
            String::new()
        }

        fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
            self.paths.clone()
        }
    }

    struct CommandTool {
        command: String,
        paths: Vec<PathBuf>,
    }

    impl Tool for CommandTool {
        fn name(&self) -> &'static str {
            "command_tool"
        }

        fn parse_input(&self, _input: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok("command_tool".to_string(), String::new(), String::new())
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn desc(&self) -> String {
            "command tool".to_string()
        }

        fn get_input(&self) -> String {
            String::new()
        }

        fn get_command(&self, _request: &dyn Request) -> Option<String> {
            Some(self.command.clone())
        }

        fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
            self.paths.clone()
        }
    }

    #[test]
    fn read_only_within_project_root_skips_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");
        std::fs::write(&file_path, "data").unwrap();

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::Ask,
                ..PermissionConfig::default()
            },
            prompter,
        );
        let request = TestRequest { root };
        let tool = ReadOnlyTool {
            paths: vec![file_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn read_only_outside_project_root_prompts() {
        let root_dir = tempfile::tempdir().unwrap();
        let external_dir = tempfile::tempdir().unwrap();
        let root = root_dir.path().to_path_buf();
        let external_file = external_dir.path().join("external.txt");
        std::fs::write(&external_file, "data").unwrap();

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::Ask,
                ..PermissionConfig::default()
            },
            prompter,
        );
        let request = TestRequest { root };
        let tool = ReadOnlyTool {
            paths: vec![external_file],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn non_read_only_tool_prompts() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");
        std::fs::write(&file_path, "data").unwrap();

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::Ask,
                ..PermissionConfig::default()
            },
            prompter,
        );
        let request = TestRequest { root };
        let tool = WriteTool {
            paths: vec![file_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn command_permission_takes_precedence() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");
        std::fs::write(&file_path, "data").unwrap();

        let command_permission = Permission::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            None,
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let tool_permission = Permission::new(
            "command_tool".to_string(),
            None,
            None,
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            Some(tool_permission),
            Some(command_permission),
            Some(path_permission),
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker =
            PermissionChecker::new_with_prompter(store, PermissionConfig::default(), prompter);
        let request = TestRequest { root };
        let tool = CommandTool {
            command: "rm -rf /".to_string(),
            paths: vec![file_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(!allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn path_permission_used_when_no_command_match() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");
        std::fs::write(&file_path, "data").unwrap();

        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            None,
            None,
            Some(path_permission),
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker =
            PermissionChecker::new_with_prompter(store, PermissionConfig::default(), prompter);
        let request = TestRequest { root };
        let tool = CommandTool {
            command: "echo hi".to_string(),
            paths: vec![file_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(!allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn tool_permission_used_when_no_command_or_path_match() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");
        std::fs::write(&file_path, "data").unwrap();

        let tool_permission = Permission::new(
            "command_tool".to_string(),
            None,
            None,
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            Some(tool_permission),
            None,
            None,
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker =
            PermissionChecker::new_with_prompter(store, PermissionConfig::default(), prompter);
        let request = TestRequest { root };
        let tool = CommandTool {
            command: "echo hi".to_string(),
            paths: vec![file_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dangerous_command_forces_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AlwaysAllow,
                ..PermissionConfig::default()
            },
            prompter,
        );
        let request = TestRequest { root };
        let tool = CommandTool {
            command: "rm -rf /".to_string(),
            paths: vec![],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn restricted_path_forces_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let restricted_path = PathBuf::from("/etc");

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AlwaysAllow,
                ..PermissionConfig::default()
            },
            prompter,
        );
        let request = TestRequest { root };
        let tool = CommandTool {
            command: "cat /etc/hosts".to_string(),
            paths: vec![restricted_path],
        };

        let allowed = checker.check(&tool, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn resolve_permission_returns_ask_for_read_only() {
        let abs = std::fs::canonicalize(".").expect("failed to canonicalize path");

        let store = Arc::new(TestStore::with_permissions(None, None, None));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "read_objects".to_string(),
            Some("".to_string()),
            vec![abs],
            PermissionScope::Project,
            Some(1),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn resolve_permission_prefers_command() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");

        let command_permission = Permission::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            None,
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            None,
            Some(command_permission),
            Some(path_permission),
        ));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            vec![file_path],
            PermissionScope::Project,
            Some(1),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::AlwaysDeny);
    }

    #[test]
    fn resolve_permission_uses_path_when_no_command() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");

        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            PermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            None,
            None,
            Some(path_permission),
        ));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![file_path],
            PermissionScope::Project,
            Some(1),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::AlwaysAllow);
    }

    #[test]
    fn resolve_permission_uses_tool_when_no_command_or_path() {
        let tool_permission = Permission::new(
            "command_tool".to_string(),
            None,
            None,
            PermissionDecision::AlwaysDeny,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(
            Some(tool_permission),
            None,
            None,
        ));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![],
            PermissionScope::Project,
            Some(1),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::AlwaysDeny);
    }

    #[test]
    fn resolve_permission_falls_back_to_default_rules() {
        let store = Arc::new(TestStore::new());
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AlwaysAllow,
                ..PermissionConfig::default()
            },
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("echo ok".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::AlwaysAllow);
    }
}
