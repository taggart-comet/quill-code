#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoList {
    pub id: Option<i64>,
    pub session_id: i64,
    pub content: Value,
}

impl TodoList {
    /// Converts the TODO list to a JSON string for display or LLM context
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.content).unwrap_or_default()
    }
}
