use super::{InferenceEngine, LLMInferenceResult};
use crate::domain::ModelType;
use crate::infrastructure::api_clients::openai::client::AuthToken;
use crate::infrastructure::api_clients::openai::OpenAIClient;
use crate::infrastructure::InfaError;
use openai_api_rust::models::ModelsApi;
use openai_api_rust::*;
use std::sync::Arc;

pub struct OpenAIEngine {
    client: OpenAI,
    responses_client: OpenAIClient,
}

impl OpenAIEngine {
    /// Create a new OpenAI engine with the given auth token and model
    pub fn new(auth_token: AuthToken, model: &str) -> Result<Arc<dyn InferenceEngine>, String> {
        // Extract API key string for openai_api_rust client (used for model listing)
        let api_key_str = match &auth_token {
            AuthToken::ApiKey(key) => key.clone(),
            AuthToken::OAuth { token, .. } => token.clone(),
        };

        let auth = Auth::new(&api_key_str);
        let client = OpenAI::new(auth, "https://api.openai.com/v1/");
        let responses_client = OpenAIClient::new(auth_token, model.to_string());

        let engine = Arc::new(Self {
            client,
            responses_client,
        });

        Ok(engine as Arc<dyn InferenceEngine>)
    }

    /// Create a new OpenAI engine with the given auth token and model
    pub fn new_general(auth_token: AuthToken, model: &str) -> Result<Self, String> {
        // Extract API key string for openai_api_rust client (used for model listing)
        let api_key_str = match &auth_token {
            AuthToken::ApiKey(key) => key.clone(),
            AuthToken::OAuth { token, .. } => token.clone(),
        };

        let auth = Auth::new(&api_key_str);
        let client = OpenAI::new(auth, "https://api.openai.com/v1/");
        let responses_client = OpenAIClient::new(auth_token, model.to_string());

        let engine = Self {
            client,
            responses_client,
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
    fn generate(
        &self,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, InfaError> {
        self.generate_with_responses_api(tools, chain, images, tracer)
    }
    fn get_type(&self) -> ModelType {
        ModelType::OpenAI
    }
}

impl OpenAIEngine {
    /// Generate using the Responses API (for newer models like codex, o-series)
    fn generate_with_responses_api(
        &self,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, InfaError> {
        self.responses_client
            .call_responses_api(tools, chain, images, tracer)
            .map_err(|e| e)
    }
}
