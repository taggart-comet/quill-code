use crate::domain::session::Request;
use crate::domain::tools::{short_words, Tool, ToolResult, TOOL_OUTPUT_BUDGET_CHARS};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct WebSearch {
    input: Mutex<Option<WebSearchInput>>,
}

#[derive(Debug, Clone)]
struct WebSearchInput {
    raw: String,
    query: String,
    max_results: u32,
    call_id: String,
}

#[derive(Debug, Deserialize)]
struct WebSearchInputJson {
    query: String,
    max_results: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchOutput {
    pub results: Vec<SearchResult>,
}

impl WebSearch {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(None),
        }
    }

    fn load_input(&self) -> Result<WebSearchInput, String> {
        let guard = self
            .input
            .lock()
            .map_err(|_| "Input lock poisoned".to_string())?;
        guard
            .clone()
            .ok_or_else(|| "Missing input for web_search".to_string())
    }

    fn permission_target_from_query(query: &str) -> Option<String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }

        Self::extract_hostname(trimmed).or_else(|| Some(format!("search:{}", trimmed)))
    }

    fn extract_hostname(query: &str) -> Option<String> {
        let url_re = Regex::new(r"(?i)https?://([a-z0-9.-]+)").ok()?;
        if let Some(caps) = url_re.captures(query) {
            if let Some(host) = caps.get(1) {
                return Some(host.as_str().to_string());
            }
        }

        let site_re = Regex::new(r"(?i)\bsite:([a-z0-9.-]+)").ok()?;
        if let Some(caps) = site_re.captures(query) {
            if let Some(host) = caps.get(1) {
                return Some(host.as_str().to_string());
            }
        }

        let domain_re = Regex::new(r"(?i)\b([a-z0-9-]+(?:\.[a-z0-9-]+)+)\b").ok()?;
        for caps in domain_re.captures_iter(query) {
            if let Some(host) = caps.get(1) {
                return Some(host.as_str().to_string());
            }
        }

        None
    }
}

impl Tool for WebSearch {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn parse_input(&self, input: String, call_id: String) -> Option<crate::domain::tools::Error> {
        let trimmed = input.trim();
        match serde_json::from_str::<WebSearchInputJson>(trimmed) {
            Ok(parsed) => {
                let max_results = parsed.max_results.unwrap_or(5).max(1).min(10);
                let parsed_input = WebSearchInput {
                    raw: input,
                    query: parsed.query,
                    max_results,
                    call_id,
                };
                *self.input.lock().unwrap() = Some(parsed_input);
                None
            }
            Err(e) => Some(crate::domain::tools::Error::Parse(format!(
                "Failed to parse web_search input: {}",
                e
            ))),
        }
    }

    fn work(&self, request: &dyn Request) -> ToolResult {
        let input = match self.load_input() {
            Ok(input) => input,
            Err(e) => {
                return ToolResult::error(self.name().to_string(), String::new(), e, String::new());
            }
        };

        // Check if web search is enabled
        let settings = match request.user_settings() {
            Some(settings) => settings,
            None => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    "User settings not available".to_string(),
                    input.call_id.clone(),
                );
            }
        };

        if !settings.web_search_enabled() {
            return ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                "Web search is not enabled in settings".to_string(),
                input.call_id.clone(),
            );
        }

        let api_key = match settings.brave_api_key() {
            Some(key) if !key.trim().is_empty() => key,
            _ => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    "Brave API key not configured".to_string(),
                    input.call_id.clone(),
                );
            }
        };

        // Perform the search
        match self.perform_search(&input.query, input.max_results, api_key) {
            Ok(results) => {
                let output = WebSearchOutput { results };
                match serde_json::to_string(&output) {
                    Ok(json_output) => {
                        ToolResult::ok(self.name().to_string(), input.raw.clone(), json_output, input.call_id)
                    }
                    Err(e) => ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        format!("Failed to serialize output: {}", e),
                        input.call_id.clone(),
                    ),
                }
            }
            Err(e) => ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                format!("Search failed: {}", e),
                input.call_id,
            ),
        }
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, excessive results may be truncated)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    fn desc(&self) -> String {
        "Use `web_search` to search things on the web using and find useful libraries and documentation to help implementation.".to_string()
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
        let query = self
            .input
            .lock()
            .unwrap()
            .as_ref()
            .map(|input| input.query.clone())
            .unwrap_or_default();
        let label = short_words(&query, 3);
        if label.is_empty() {
            "Searching web".to_string()
        } else {
            format!("Searching {}", label)
        }
    }

    fn get_command(&self, _request: &dyn Request) -> Option<String> {
        None
    }

    fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
        let query = self
            .input
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|input| input.query.clone()));

        query
            .as_deref()
            .and_then(Self::permission_target_from_query)
            .map(|target| vec![PathBuf::from(target)])
            .unwrap_or_default()
    }
}

impl WebSearch {
    fn perform_search(
        &self,
        query: &str,
        max_results: u32,
        api_key: &str,
    ) -> Result<Vec<SearchResult>, String> {
        let client = Client::new();
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding::encode(query),
            max_results
        );

        let response = client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "API request failed with status: {}",
                response.status()
            ));
        }

        let response_json: Value = response
            .json()
            .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

        let mut results = Vec::new();

        if let Some(web_results) = response_json.get("web").and_then(|w| w.get("results")) {
            if let Some(results_array) = web_results.as_array() {
                for result in results_array {
                    if let (Some(title), Some(url), Some(description)) = (
                        result.get("title").and_then(|t| t.as_str()),
                        result.get("url").and_then(|u| u.as_str()),
                        result.get("description").and_then(|d| d.as_str()),
                    ) {
                        results.push(SearchResult {
                            title: title.to_string(),
                            url: url.to_string(),
                            snippet: description.to_string(),
                            source: "Brave Search".to_string(),
                        });
                    }

                    if results.len() >= max_results as usize {
                        break;
                    }
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::permissions::{
        checker::{PermissionChecker, PermissionPrompter},
        store::{PermissionStore, StoreError},
        types::{PermissionConfig, PermissionDecision, PermissionRequest, PermissionScope},
    };
    use crate::domain::session::{Request, SessionRequest};
    use crate::domain::UserSettings;
    use crate::repository::UserSettingsRow;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct TestStore {
        created: Mutex<Vec<crate::domain::permissions::types::Permission>>,
        tool_permission: Mutex<Option<crate::domain::permissions::types::Permission>>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                tool_permission: Mutex::new(None),
            }
        }

        fn with_tool_permission(permission: crate::domain::permissions::types::Permission) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                tool_permission: Mutex::new(Some(permission)),
            }
        }
    }

    impl PermissionStore for TestStore {
        fn create_permission(
            &self,
            permission: crate::domain::permissions::types::Permission,
        ) -> Result<crate::domain::permissions::types::Permission, StoreError> {
            self.created.lock().unwrap().push(permission.clone());
            Ok(permission)
        }

        fn find_session_permissions(&self, _project_id: i32) -> Result<Vec<crate::domain::permissions::types::Permission>, StoreError> {
            // Return empty for tests - this test doesn't use session permissions
            Ok(Vec::new())
        }

        fn find_permission(
            &self,
            tool: &str,
            project_id: i32,
            command_pattern: &str,
            resource_pattern: &str,
        ) -> Result<Option<crate::domain::permissions::types::Permission>, StoreError> {
            let permission = self.tool_permission.lock().unwrap().clone();
            if let Some(permission) = permission {
                // Check if project_id matches
                if let Some(perm_project_id) = permission.project_id {
                    if perm_project_id != project_id {
                        return Ok(None);
                    }
                }

                // Check if tool matches
                if permission.tool_name != tool {
                    return Ok(None);
                }

                // Check command pattern match
                let command_matches = match &permission.command_pattern {
                    Some(pattern) => pattern == command_pattern,
                    None => command_pattern.is_empty(),
                };

                // Check resource pattern match
                let resource_matches = match &permission.resource_pattern {
                    Some(pattern) => pattern == resource_pattern,
                    None => resource_pattern.is_empty(),
                };

                if command_matches && resource_matches {
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
        ) -> Result<PermissionDecision, crate::utils::AskError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.decision.clone())
        }
    }

    struct TestRequest {
        root: PathBuf,
        user_settings: Option<UserSettings>,
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

        fn user_settings(&self) -> Option<&UserSettings> {
            self.user_settings.as_ref()
        }

        fn project_id(&self) -> Option<i32> {
            Some(1)
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

    fn make_mock_user_settings(
        web_search_enabled: bool,
        brave_api_key: Option<&str>,
    ) -> UserSettings {
        let row = UserSettingsRow {
            id: 1,
            openai_api_key: None,
            openai_tracing_enabled: false,
            use_behavior_trees: false,
            current_model_id: None,
            web_search_enabled,
            brave_api_key: brave_api_key.map(String::from),
            max_tool_calls_per_request: 10,
        };
        UserSettings::from(row)
    }

    #[test]
    fn web_search_prompts_permission_when_not_previously_authorized() {
        let root = PathBuf::from("/tmp");
        let user_settings = Some(make_mock_user_settings(true, Some("test-key")));

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

        let request = TestRequest {
            root,
            user_settings,
        };
        let web_search = WebSearch::new();

        let allowed = checker.check(&web_search, &request, Some(1)).unwrap();

        assert!(allowed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn web_search_permission_request_includes_query_hostname() {
        let root = PathBuf::from("/tmp");
        let user_settings = Some(make_mock_user_settings(true, Some("test-key")));

        let store = Arc::new(TestStore::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let decision_to_return = Arc::new(Mutex::new(PermissionDecision::AllowOnce));
        let captured_request = Arc::new(Mutex::new(None::<PermissionRequest>));

        struct CapturingPrompter {
            calls: Arc<AtomicUsize>,
            decision: Arc<Mutex<PermissionDecision>>,
            captured: Arc<Mutex<Option<PermissionRequest>>>,
        }

        impl PermissionPrompter for CapturingPrompter {
            fn ask_permission(
                &self,
                request: &PermissionRequest,
            ) -> Result<PermissionDecision, crate::utils::AskError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                *self.captured.lock().unwrap() = Some(request.clone());
                Ok(self.decision.lock().unwrap().clone())
            }
        }

        let prompter = Arc::new(CapturingPrompter {
            calls: Arc::clone(&calls),
            decision: Arc::clone(&decision_to_return),
            captured: Arc::clone(&captured_request),
        });

        let checker = PermissionChecker::new_with_prompter(
            store,
            PermissionConfig {
                default_decision: PermissionDecision::Ask,
                ..PermissionConfig::default()
            },
            prompter,
        );

        let request = TestRequest {
            root,
            user_settings,
        };
        let web_search = WebSearch::new();
        let _ = web_search.parse_input(
            r#"{"query":"site:docs.rs serde","max_results":5}"#.to_string(),
            "call-id".to_string(),
        );

        let _allowed = checker.check(&web_search, &request, Some(1)).unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let captured_req = captured_request.lock().unwrap().as_ref().unwrap().clone();
        assert_eq!(captured_req.tool_name, "web_search");
        assert_eq!(captured_req.paths, vec![PathBuf::from("docs.rs")]);
    }

}