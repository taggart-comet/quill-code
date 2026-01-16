use super::InferenceEngine;
use crate::domain::ModelType;
use crate::infrastructure::api_clients::openai::OpenAIClient;
use openai_api_rust::completions::*;
use openai_api_rust::models::ModelsApi;
use openai_api_rust::*;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};

pub struct OpenAIEngine {
    client: OpenAI,
    responses_client: OpenAIClient,
    model: String,
}

impl OpenAIEngine {
    /// Create a new OpenAI engine with the given API key and model
    pub fn new(api_key: &str, model: &str) -> Result<Arc<dyn InferenceEngine>, String> {
        let auth = Auth::new(api_key);
        let client = OpenAI::new(auth, "https://api.openai.com/v1/");
        let responses_client = OpenAIClient::new(api_key.to_string(), model.to_string());

        let engine = Arc::new(Self {
            client,
            responses_client,
            model: model.to_string(),
        });

        Ok(engine as Arc<dyn InferenceEngine>)
    }

    /// Create a new OpenAI engine with the given API key and model
    pub fn new_general(api_key: &str, model: &str) -> Result<Self, String> {
        let auth = Auth::new(api_key);
        let client = OpenAI::new(auth, "https://api.openai.com/v1/");
        let responses_client = OpenAIClient::new(api_key.to_string(), model.to_string());

        let engine = Self {
            client,
            responses_client,
            model: model.to_string(),
        };

        Ok(engine)
    }

    /// Fetch available models from OpenAI API
    pub fn fetch_available_models(&self) -> Result<Vec<String>, String> {
        let response = self
            .client
            .models_list()
            .map_err(|e| format!("Failed to fetch models: {}", e))?;

        let mut model_ids: Vec<String> = response
            .iter()
            .filter_map(|model| Some(model.id.clone()))
            .filter(|id| {
                id.starts_with("gpt-")
                    || id.starts_with("o1")
                    || id.starts_with("o3")
                    || id.starts_with("o4")
            })
            .collect();

        model_ids.sort();
        Ok(model_ids)
    }
}

impl InferenceEngine for OpenAIEngine {
    fn generate(&self, prompt: &str, _max_tokens: u32) -> Result<String, String> {
        match self.generate_with_responses_api(prompt) {
            Ok(result) => Ok(result),
            Err(e) => Err(e),
        }
    }
    fn get_type(&self) -> ModelType {
        ModelType::OpenAI
    }
}

impl OpenAIEngine {
    /// Generate using the Responses API (for newer models like codex, o-series)
    fn generate_with_responses_api(&self, prompt: &str) -> Result<String, String> {
        self.responses_client
            .call_responses_api(prompt)
            .map_err(|e| format!("OpenAI API error: {}", e))
    }
}
