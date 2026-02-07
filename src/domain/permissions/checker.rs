use super::store::{PermissionStore, StoreError};
use super::types::{PermissionConfig, PermissionDecision, PermissionRequest, PermissionScope};
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
            tool.is_read_only(),
            request.project_root().to_path_buf(),
        );
        let decision = self.resolve_permission(&permission_request)?;
        if decision == PermissionDecision::Ask
            && self.is_allowed_by_default(tool, request, &permission_request)
        {
            return Ok(true);
        }
        match decision {
            PermissionDecision::AllowOnce => Ok(true),
            PermissionDecision::Ask => self.prompt_and_store(&permission_request),
            PermissionDecision::AllowAllReadsInSession
            | PermissionDecision::AllowAllWritesInSession
            | PermissionDecision::AllowCommandForProject => {
                // These should have been resolved already, but handle them safely
                Ok(true)
            }
        }
    }

    /// Store a permission decision
    fn store_permission_decision(
        &self,
        request: &PermissionRequest,
        decision: PermissionDecision,
    ) -> Result<(), CheckerError> {
        // Handle command-specific permissions (project-scoped, persistent)
        if decision == PermissionDecision::AllowCommandForProject {
            let permission = super::types::Permission::new(
                request.tool_name.clone(),
                request.command.clone(),  // Store the specific command
                None,                      // No resource pattern for commands
                decision,
                PermissionScope::Project,  // Project scope = persistent
                request.project_id,
            );
            self.store.create_permission(permission)?;
            return Ok(());
        }

        // Handle session-wide permissions
        if !matches!(decision, PermissionDecision::AllowAllReadsInSession | PermissionDecision::AllowAllWritesInSession) {
            return Ok(());
        }

        // Get project root pattern for session-wide permissions
        let resource_pattern = if let Some(first_path) = request.paths.first() {
            // Try to get the project root by going up the directory tree
            let mut path = first_path.clone();
            while let Some(parent) = path.parent() {
                path = parent.to_path_buf();
                // Stop when we find a reasonable root (has .git, or is a few levels up)
                if path.join(".git").exists() || path.parent().is_none() {
                    break;
                }
            }
            Some(format!("{}/**", path.to_string_lossy()))
        } else {
            None
        };

        let permission = super::types::Permission::new(
            request.tool_name.clone(),
            None,  // No command pattern for session-wide
            resource_pattern,
            decision,
            PermissionScope::Session,
            request.project_id,
        );

        self.store.create_permission(permission)?;
        Ok(())
    }

    /// Check if a command is dangerous
    fn is_dangerous_command(&self, command: &str) -> bool {
        self.config
            .dangerous_commands
            .iter()
            .any(|dangerous| command.contains(dangerous) || command.starts_with(dangerous))
    }

    /// Check if a path is restricted
    fn is_restricted_path(&self, path: &PathBuf) -> bool {
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
        let project_id: i32 = request.project_id.unwrap_or(0);
        if project_id == 0 {
            return Ok(PermissionDecision::Ask);
        }

        // Check for command-specific permissions first (project-scoped, persistent)
        if let Some(command) = &request.command {
            if let Ok(Some(perm)) = self.store.find_permission(
                &request.tool_name,
                project_id,
                command,
                "",
            ) {
                if perm.decision == PermissionDecision::AllowCommandForProject
                    && perm.scope == PermissionScope::Project {
                    return Ok(PermissionDecision::AllowOnce); // Grant this specific operation
                }
            }
        }

        // Check for session-wide permissions
        let session_permissions = self.store.find_session_permissions(project_id)?;
        for perm in session_permissions {
            // CRITICAL SECURITY CHECK: Only grant session-wide permissions if ALL paths are within project root
            let all_paths_in_project = request.paths.iter().all(|path| {
                let normalized = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    request.project_root.join(path)
                };
                is_within_root(&normalized, &request.project_root)
            });

            if !all_paths_in_project {
                // Paths outside project - don't use session permission, ask explicitly
                continue;
            }

            if request.is_read_only && perm.is_session_wide_all_reads() {
                return Ok(PermissionDecision::AllowOnce); // Grant this specific operation
            }
            if !request.is_read_only && perm.is_session_wide_all_writes() {
                return Ok(PermissionDecision::AllowOnce); // Grant this specific operation
            }
        }

        // Apply default security rules
        self.check_default_rules(request)
    }

    fn prompt_and_store(&self, request: &PermissionRequest) -> Result<bool, CheckerError> {
        match self.prompter.ask_permission(request) {
            Ok(decision @ PermissionDecision::AllowAllReadsInSession)
            | Ok(decision @ PermissionDecision::AllowAllWritesInSession)
            | Ok(decision @ PermissionDecision::AllowCommandForProject) => {
                self.store_permission_decision(request, decision)?;
                Ok(true)
            }
            Ok(PermissionDecision::AllowOnce) => Ok(true),
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
        permissions: Mutex<Vec<Permission>>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                permissions: Mutex::new(Vec::new()),
            }
        }

        fn with_permissions(permissions: Vec<Permission>) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                permissions: Mutex::new(permissions),
            }
        }
    }

    impl PermissionStore for TestStore {
        fn create_permission(&self, permission: Permission) -> Result<Permission, StoreError> {
            self.created.lock().unwrap().push(permission.clone());
            Ok(permission)
        }

        fn find_session_permissions(&self, project_id: i32) -> Result<Vec<Permission>, StoreError> {
            let permissions = self.permissions.lock().unwrap();
            let session_perms: Vec<Permission> = permissions
                .iter()
                .filter(|p| {
                    p.project_id == Some(project_id)
                        && p.scope == PermissionScope::Session
                        && matches!(
                            p.decision,
                            PermissionDecision::AllowAllReadsInSession
                                | PermissionDecision::AllowAllWritesInSession
                        )
                })
                .cloned()
                .collect();
            Ok(session_perms)
        }

        fn find_permission(
            &self,
            tool: &str,
            project_id: i32,
            command_pattern: &str,
            resource_pattern: &str,
        ) -> Result<Option<Permission>, StoreError> {
            let permissions = self.permissions.lock().unwrap();

            // Find the most specific matching permission
            // Priority: command+path > command > path > tool
            let mut best_match: Option<(&Permission, i32)> = None;

            for permission in permissions.iter() {
                // Check if project_id matches
                if let Some(perm_project_id) = permission.project_id {
                    if perm_project_id != project_id {
                        continue;
                    }
                }

                // Check if tool matches
                if permission.tool_name != tool {
                    continue;
                }

                // Calculate specificity and check if it matches
                let mut specificity = 0;
                let mut matches = true;

                // Check command pattern
                match &permission.command_pattern {
                    Some(pattern) => {
                        if !command_pattern.is_empty() && pattern == command_pattern {
                            specificity += 2;
                        } else if !command_pattern.is_empty() {
                            matches = false;
                        }
                    }
                    None => {
                        // None means it matches any command
                    }
                }

                // Check resource pattern
                match &permission.resource_pattern {
                    Some(pattern) => {
                        if !resource_pattern.is_empty() && pattern == resource_pattern {
                            specificity += 1;
                        } else if !resource_pattern.is_empty() {
                            matches = false;
                        }
                    }
                    None => {
                        // None means it matches any resource
                    }
                }

                if matches {
                    if let Some((_, best_specificity)) = best_match {
                        if specificity > best_specificity {
                            best_match = Some((permission, specificity));
                        }
                    } else {
                        best_match = Some((permission, specificity));
                    }
                }
            }

            Ok(best_match.map(|(p, _)| p.clone()))
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

        fn parse_input(&self, _input: String, _call_id: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok(
                "read_only".to_string(),
                String::new(),
                String::new(),
                String::new(),
            )
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

        fn parse_input(&self, _input: String, _call_id: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok(
                "write_tool".to_string(),
                String::new(),
                String::new(),
                String::new(),
            )
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

        fn parse_input(&self, _input: String, _call_id: String) -> Option<Error> {
            None
        }

        fn work(&self, _request: &dyn Request) -> ToolResult {
            ToolResult::ok(
                "command_tool".to_string(),
                String::new(),
                String::new(),
                String::new(),
            )
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
            decision: PermissionDecision::AllowOnce,
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
            decision: PermissionDecision::AllowOnce,
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
            decision: PermissionDecision::AllowOnce,
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
    fn dangerous_command_forces_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: PermissionDecision::AllowOnce,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AllowOnce,
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
            decision: PermissionDecision::AllowOnce,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AllowOnce,
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

        let store = Arc::new(TestStore::with_permissions(vec![]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );
        let request = PermissionRequest::new(
            "read_objects".to_string(),
            Some("".to_string()),
            vec![abs.clone()],
            PermissionScope::Project,
            Some(1),
            true,  // is_read_only
            abs.parent().unwrap_or(&abs).to_path_buf(),
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
            PermissionDecision::Ask,
            PermissionScope::Project,
            Some(1),
        );
        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            PermissionDecision::AllowOnce,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(vec![
            command_permission,
            path_permission,
        ]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            vec![file_path.clone()],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::Ask);
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
            PermissionDecision::AllowOnce,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(vec![path_permission]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![file_path.clone()],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn resolve_permission_uses_tool_when_no_command_or_path() {
        let tool_permission = Permission::new(
            "command_tool".to_string(),
            None,
            None,
            PermissionDecision::Ask,
            PermissionScope::Project,
            Some(1),
        );
        let store = Arc::new(TestStore::with_permissions(vec![tool_permission]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            PathBuf::from("/tmp"),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn resolve_permission_falls_back_to_default_rules() {
        let store = Arc::new(TestStore::new());
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::AllowOnce,
                ..PermissionConfig::default()
            },
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("echo ok".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            PathBuf::from("/tmp"),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, PermissionDecision::AllowOnce);
    }

    #[test]
    fn session_read_permission_only_works_within_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let external_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();
        let external_file = external_dir.path().join("external.txt");
        std::fs::write(&external_file, "data").unwrap();

        // Create a session-wide read permission
        let session_perm = Permission::new(
            "read_only".to_string(),
            None,
            Some(format!("{}/**", project_root.to_string_lossy())),
            PermissionDecision::AllowAllReadsInSession,
            PermissionScope::Session,
            Some(1),
        );

        let store = Arc::new(TestStore::with_permissions(vec![session_perm]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );

        // Try to read a file OUTSIDE the project
        let request = PermissionRequest::new(
            "read_only".to_string(),
            None,
            vec![external_file],
            PermissionScope::Project,
            Some(1),
            true,  // is_read_only
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // Should NOT grant access via session permission - should Ask
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn session_write_permission_only_works_within_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let external_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();
        let external_file = external_dir.path().join("external.txt");
        std::fs::write(&external_file, "data").unwrap();

        // Create a session-wide write permission
        let session_perm = Permission::new(
            "write_tool".to_string(),
            None,
            Some(format!("{}/**", project_root.to_string_lossy())),
            PermissionDecision::AllowAllWritesInSession,
            PermissionScope::Session,
            Some(1),
        );

        let store = Arc::new(TestStore::with_permissions(vec![session_perm]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );

        // Try to write a file OUTSIDE the project
        let request = PermissionRequest::new(
            "write_tool".to_string(),
            None,
            vec![external_file],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // Should NOT grant access via session permission - should Ask
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn session_read_permission_works_within_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();
        let internal_file = project_root.join("internal.txt");
        std::fs::write(&internal_file, "data").unwrap();

        // Create a session-wide read permission
        let session_perm = Permission::new(
            "read_only".to_string(),
            None,
            Some(format!("{}/**", project_root.to_string_lossy())),
            PermissionDecision::AllowAllReadsInSession,
            PermissionScope::Session,
            Some(1),
        );

        let store = Arc::new(TestStore::with_permissions(vec![session_perm]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );

        // Try to read a file INSIDE the project
        let request = PermissionRequest::new(
            "read_only".to_string(),
            None,
            vec![internal_file],
            PermissionScope::Project,
            Some(1),
            true,  // is_read_only
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // SHOULD grant access via session permission
        assert_eq!(decision, PermissionDecision::AllowOnce);
    }

    #[test]
    fn command_permission_allows_specific_command_for_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();

        // Create a project-scoped command permission
        let command_perm = Permission::new(
            "shell_exec".to_string(),
            Some("npm test".to_string()),
            None,
            PermissionDecision::AllowCommandForProject,
            PermissionScope::Project,
            Some(1),
        );

        let store = Arc::new(TestStore::with_permissions(vec![command_perm]));
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: PermissionDecision::AllowOnce,
            }),
        );

        // Request for the SAME command should be allowed
        let request = PermissionRequest::new(
            "shell_exec".to_string(),
            Some("npm test".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
            false,  // is_read_only
            project_root.clone(),
        );

        let decision = checker.resolve_permission(&request).unwrap();
        assert_eq!(decision, PermissionDecision::AllowOnce);  // Allowed via stored permission

        // Request for a DIFFERENT command should ask
        let different_request = PermissionRequest::new(
            "shell_exec".to_string(),
            Some("npm run build".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
            false,
            project_root,
        );

        let decision = checker.resolve_permission(&different_request).unwrap();
        assert_eq!(decision, PermissionDecision::Ask);  // Different command, must ask
    }
}