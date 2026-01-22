use crate::domain::session::Request;
use crate::domain::tools::{Tool, ToolResult};
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
        let guard = self.input.lock().map_err(|_| "Input lock poisoned".to_string())?;
        guard
            .clone()
            .ok_or_else(|| "Missing input for web_search".to_string())
    }
}

impl Tool for WebSearch {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn parse_input(&self, input: String) -> Option<crate::domain::tools::Error> {
        let trimmed = input.trim();
        match serde_json::from_str::<WebSearchInputJson>(trimmed) {
            Ok(parsed) => {
                let max_results = parsed.max_results.unwrap_or(5).max(1).min(10);
                let parsed_input = WebSearchInput {
                    raw: input,
                    query: parsed.query,
                    max_results,
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
                return ToolResult::error(self.name().to_string(), String::new(), e);
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
                );
            }
        };

        if !settings.web_search_enabled() {
            return ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                "Web search is not enabled in settings".to_string(),
            );
        }

        let api_key = match settings.brave_api_key() {
            Some(key) if !key.trim().is_empty() => key,
            _ => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    "Brave API key not configured".to_string(),
                );
            }
        };

        // Perform the search
        match self.perform_search(&input.query, input.max_results, api_key) {
            Ok(results) => {
                let output = WebSearchOutput { results };
                match serde_json::to_string(&output) {
                    Ok(json_output) => ToolResult::ok(
                        self.name().to_string(),
                        input.raw.clone(),
                        json_output,
                    ),
                    Err(e) => ToolResult::error(
                        self.name().to_string(),
                        input.raw.clone(),
                        format!("Failed to serialize output: {}", e),
                    ),
                }
            }
            Err(e) => ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                format!("Search failed: {}", e),
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
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    fn desc(&self) -> String {
        "Search the web using Brave Search API. Requires web search to be enabled in settings with a valid Brave API key.".to_string()
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
        None
    }

    fn get_affected_paths(&self, _request: &dyn Request) -> Vec<PathBuf> {
        // Return the Brave API hostname for permission checking
        vec![PathBuf::from("api.search.brave.com")]
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
