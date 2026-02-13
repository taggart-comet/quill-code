use super::store::{PermissionStore, StoreError};
use super::types::{
    PermissionConfig, PermissionRequest, PermissionScope, SystemPermissionDecision,
    UserPermissionDecision,
};
use crate::domain::session::Request;
use crate::domain::tools::Tool;
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
    fn ask_permission(
        &self,
        request: &PermissionRequest,
    ) -> Result<UserPermissionDecision, AskError>;
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

        // Auto-allow read-only operations within project root
        if tool.is_read_only() {
            let all_within_project = tool.get_affected_paths(request)
                .iter()
                .all(|p| p.starts_with(&request.project_root().to_path_buf()));
            if all_within_project {
                return Ok(true);
            }
        }
        
        let permission_request = PermissionRequest::new(
            tool.name().to_string(),
            tool.get_command(request),
            tool.get_affected_paths(request),
            PermissionScope::Project,
            project_id,
            tool.is_read_only(),
            request.project_root().to_path_buf(),
        );

        // Dangerous commands always require a prompt
        if let Some(ref cmd) = permission_request.command {
            if self.is_dangerous_command(cmd) {
                return self.prompt_and_store(&permission_request);
            }
        }

        // Restricted paths always require a prompt
        if permission_request
            .paths
            .iter()
            .any(|p| self.is_restricted_path(p))
        {
            return self.prompt_and_store(&permission_request);
        }

        let decision = self.resolve_permission(&permission_request)?;
        match decision {
            SystemPermissionDecision::Allow => Ok(true),
            SystemPermissionDecision::Ask => self.prompt_and_store(&permission_request),
        }
    }

    /// Store a permission decision
    fn store_permission_decision(
        &self,
        request: &PermissionRequest,
        user_decision: UserPermissionDecision,
    ) -> Result<(), CheckerError> {
        if user_decision == UserPermissionDecision::AllowOnce {
            return Ok(());
        }

        // Minimal behavior: for project-scoped patch_files AlwaysAllow, store without
        // resource pattern so it applies to any path within the project for this tool.
        let resource_pattern = if request.tool_name == "patch_files"
            && user_decision == UserPermissionDecision::AlwaysAllow
            && request.scope == PermissionScope::Project
        {
            None
        } else if let Some(first_path) = request.paths.first() {
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
            None, // No command pattern for session-wide
            resource_pattern,
            user_decision,
            PermissionScope::Project,
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

    fn resolve_permission(
        &self,
        request: &PermissionRequest,
    ) -> Result<SystemPermissionDecision, CheckerError> {
        let project_id: i32 = request.project_id.unwrap_or(0);
        if project_id == 0 {
            return Ok(SystemPermissionDecision::Ask);
        }

        let command_str = request.command.as_ref().map_or("", |s| s.as_str());
        let path_str = request
            .paths
            .first()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let perm =
            self.store
                .find_permission(&request.tool_name, project_id, command_str, &path_str);

        if let Ok(Some(p)) = perm {
            return Ok(p.system_decision());
        }

        if request.tool_name == "patch_files" {
            if let Ok(Some(p)) = self
                .store
                .find_permission(&request.tool_name, project_id, "", "")
            {
                return Ok(p.system_decision());
            }
        }

        Ok(self.config.default_decision.clone())
    }

    fn prompt_and_store(&self, request: &PermissionRequest) -> Result<bool, CheckerError> {
        match self.prompter.ask_permission(request) {
            Ok(decision) => {
                self.store_permission_decision(request, decision)?;
                Ok(true)
            }
            Err(AskError::IoError) => {
                Err(CheckerError::Failed("Permission prompt failed".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::permissions::store::SqlitePermissionStore;
    use crate::domain::permissions::types::Permission;
    use crate::domain::permissions::{PermissionRequest, PermissionScope};
    use crate::domain::session::{Request, SessionRequest};
    use crate::domain::tools::{Error, Tool, ToolResult};
    use crate::infrastructure::db::DbPool;
    use r2d2_sqlite::SqliteConnectionManager;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    struct TestDb {
        _temp_dir: TempDir,
        pool: DbPool,
    }

    impl TestDb {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("permissions.db");
            let manager = SqliteConnectionManager::file(&db_path);
            let pool = r2d2::Pool::new(manager).unwrap();
            let conn = pool.get().unwrap();

            conn.execute(
                "CREATE TABLE IF NOT EXISTS permissions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    tool_name TEXT NOT NULL,
                    command_pattern TEXT,
                    resource_pattern TEXT,
                    decision TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    project_id INTEGER,
                    created_at TEXT NOT NULL
                )",
                [],
            )
            .unwrap();

            Self {
                _temp_dir: temp_dir,
                pool,
            }
        }

        fn store(&self) -> Arc<SqlitePermissionStore> {
            Arc::new(SqlitePermissionStore::new(self.pool.clone()))
        }
    }

    fn seed_permissions(store: &SqlitePermissionStore, permissions: Vec<Permission>) {
        for permission in permissions {
            store.create_permission(permission).unwrap();
        }
    }

    struct TestPrompter {
        calls: Arc<AtomicUsize>,
        decision: UserPermissionDecision,
    }

    impl PermissionPrompter for TestPrompter {
        fn ask_permission(
            &self,
            _request: &PermissionRequest,
        ) -> Result<UserPermissionDecision, AskError> {
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

        let test_db = TestDb::new();
        let store = test_db.store();
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: UserPermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Ask,
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

        let test_db = TestDb::new();
        let store = test_db.store();
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: UserPermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Ask,
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

        let test_db = TestDb::new();
        let store = test_db.store();
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: UserPermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Ask,
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

        let test_db = TestDb::new();
        let store = test_db.store();
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: UserPermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Allow,
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

        let test_db = TestDb::new();
        let store = test_db.store();
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Arc::new(TestPrompter {
            calls: Arc::clone(&calls),
            decision: UserPermissionDecision::AlwaysAllow,
        });
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Allow,
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
    fn resolve_permission_returns_ask_when_no_stored_permission() {
        let abs = std::fs::canonicalize(".").expect("failed to canonicalize path");

        let test_db = TestDb::new();
        let store = test_db.store();
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "read_objects".to_string(),
            Some("".to_string()),
            vec![abs.clone()],
            PermissionScope::Project,
            Some(1),
            true,
            abs.parent().unwrap_or(&abs).to_path_buf(),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, SystemPermissionDecision::Ask);
    }

    #[test]
    fn resolve_permission_prefers_command() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_path = root.join("sample.txt");

        // Command permission with AllowOnce → system_decision = Ask
        let command_permission = Permission::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            None,
            UserPermissionDecision::AllowOnce,
            PermissionScope::Project,
            Some(1),
        );
        // Path permission with AlwaysAllow → system_decision = Allow
        let path_permission = Permission::new(
            "command_tool".to_string(),
            None,
            Some(file_path.to_string_lossy().to_string()),
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(
            store.as_ref(),
            vec![command_permission, path_permission],
        );
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("rm -rf /".to_string()),
            vec![file_path.clone()],
            PermissionScope::Project,
            Some(1),
            false,
            root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // Command match has higher specificity, and AllowOnce → Ask
        assert_eq!(decision, SystemPermissionDecision::Ask);
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
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![path_permission]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![file_path.clone()],
            PermissionScope::Project,
            Some(1),
            false,
            root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // Path permission with AlwaysAllow → Allow
        assert_eq!(decision, SystemPermissionDecision::Allow);
    }

    #[test]
    fn resolve_permission_uses_tool_when_no_command_or_path() {
        let tool_permission = Permission::new(
            "command_tool".to_string(),
            None,
            None,
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );
        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![tool_permission]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            None,
            vec![],
            PermissionScope::Project,
            Some(1),
            false,
            PathBuf::from("/tmp"),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // AlwaysAllow → system_decision = Allow
        assert_eq!(decision, SystemPermissionDecision::Allow);
    }

    #[test]
    fn resolve_permission_falls_back_to_default_rules() {
        let test_db = TestDb::new();
        let store = test_db.store();
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: SystemPermissionDecision::Allow,
                ..PermissionConfig::default()
            },
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );
        let request = PermissionRequest::new(
            "command_tool".to_string(),
            Some("echo ok".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
            false,
            PathBuf::from("/tmp"),
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, SystemPermissionDecision::Allow);
    }

    #[test]
    fn stored_permission_does_not_apply_to_different_resource() {
        let project_dir = tempfile::tempdir().unwrap();
        let external_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();
        let external_file = external_dir.path().join("external.txt");
        std::fs::write(&external_file, "data").unwrap();

        // Create a permission with a specific resource pattern
        let perm = Permission::new(
            "read_only".to_string(),
            None,
            Some(format!("{}/**", project_root.to_string_lossy())),
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        // Request for a file that doesn't match the resource pattern
        let request = PermissionRequest::new(
            "read_only".to_string(),
            None,
            vec![external_file],
            PermissionScope::Project,
            Some(1),
            true,
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        // Resource pattern doesn't match → falls back to default (Ask)
        assert_eq!(decision, SystemPermissionDecision::Ask);
    }

    #[test]
    fn stored_permission_does_not_apply_to_different_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();

        // Create a permission for project 1
        let perm = Permission::new(
            "write_tool".to_string(),
            None,
            None,
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        // Request for project 2 - should not match
        let request = PermissionRequest::new(
            "write_tool".to_string(),
            None,
            vec![project_root.join("file.txt")],
            PermissionScope::Project,
            Some(2),
            false,
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, SystemPermissionDecision::Ask);
    }

    #[test]
    fn stored_write_permission_grants_access() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();

        let target_file = project_root.join("internal.txt");
        let perm = Permission::new(
            "write_tool".to_string(),
            None,
            Some(target_file.to_string_lossy().to_string()),
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        let request = PermissionRequest::new(
            "write_tool".to_string(),
            None,
            vec![target_file],
            PermissionScope::Project,
            Some(1),
            false,
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, SystemPermissionDecision::Allow);
    }

    #[test]
    fn stored_read_permission_grants_access() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();
        let internal_file = project_root.join("internal.txt");
        std::fs::write(&internal_file, "data").unwrap();

        // Create an AlwaysAllow permission for read_only matching a specific resource.
        let perm = Permission::new(
            "read_only".to_string(),
            None,
            Some(internal_file.to_string_lossy().to_string()),
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        let request = PermissionRequest::new(
            "read_only".to_string(),
            None,
            vec![internal_file],
            PermissionScope::Project,
            Some(1),
            true,
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();

        assert_eq!(decision, SystemPermissionDecision::Allow);
    }

    #[test]
    fn command_permission_allows_specific_command_for_project() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();

        // Create a project-scoped command permission with AlwaysAllow
        let command_perm = Permission::new(
            "shell_exec".to_string(),
            Some("npm test".to_string()),
            None,
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![command_perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        // Request for the SAME command should be allowed
        let request = PermissionRequest::new(
            "shell_exec".to_string(),
            Some("npm test".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
            false,
            project_root.clone(),
        );

        let decision = checker.resolve_permission(&request).unwrap();
        assert_eq!(decision, SystemPermissionDecision::Allow);

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
        assert_eq!(decision, SystemPermissionDecision::Ask);
    }

    #[test]
    fn patch_files_permission_within_project_root_applies_to_other_project_files() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_root = project_dir.path().to_path_buf();

        // User previously created an AlwaysAllow permission for patch_files
        // targeting one file inside this project.
        let existing_perm = Permission::new(
            "patch_files".to_string(),
            None,
            None,
            UserPermissionDecision::AlwaysAllow,
            PermissionScope::Project,
            Some(1),
        );

        let test_db = TestDb::new();
        let store = test_db.store();
        seed_permissions(store.as_ref(), vec![existing_perm]);
        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig::default(),
            Arc::new(TestPrompter {
                calls: Arc::new(AtomicUsize::new(0)),
                decision: UserPermissionDecision::AlwaysAllow,
            }),
        );

        // New request patches a DIFFERENT file, but still within the same project root.
        // Expected behavior (desired): this should be allowed by the existing permission.
        let request = PermissionRequest::new(
            "patch_files".to_string(),
            None,
            vec![project_root.join("src/b.rs")],
            PermissionScope::Project,
            Some(1),
            false,
            project_root,
        );

        let decision = checker.resolve_permission(&request).unwrap();
        assert_eq!(decision, SystemPermissionDecision::Allow);
    }
}