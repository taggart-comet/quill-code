use super::{InferenceEngine, LLMInferenceResult};
use crate::domain::ModelType;
use crate::infrastructure::api_clients::openai::client::AuthToken;
use crate::infrastructure::api_clients::openai::OpenAIClient;
use crate::infrastructure::auth::refresh_oauth_tokens;
use crate::infrastructure::InfaError;
use crate::repository::{ModelsRepository, UserSettingsRepository};
use openai_api_rust::models::ModelsApi;
use openai_api_rust::*;
use std::sync::Arc;

pub struct OpenAIEngine {
    client: OpenAI,
    responses_client: OpenAIClient,
    conn: crate::infrastructure::db::DbPool,
    #[allow(dead_code)]
    model_name: String,
}

impl OpenAIEngine {
    /// Create a new OpenAI engine with the given auth token and model
    pub fn new(
        auth_token: AuthToken,
        model: &str,
        conn: crate::infrastructure::db::DbPool,
    ) -> Result<Arc<dyn InferenceEngine>, String> {
        // Extract API key string for openai_api_rust client (used for model listing)
        let api_key_str = match &auth_token {
            AuthToken::ApiKey(key) => key.clone(),
            AuthToken::OAuth { token, .. } => token.clone(),
        };

        let auth = Auth::new(&api_key_str);
        let client = OpenAI::new(auth, "https://api.openai.com/v1/");
        let responses_client = OpenAIClient::new(model.to_string());

        let engine = Arc::new(Self {
            client,
            responses_client,
            conn,
            model_name: model.to_string(),
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
        let responses_client = OpenAIClient::new(model.to_string());

        let engine = Self {
            client,
            responses_client,
            conn: crate::infrastructure::db::init_db("quillcode")
                .map_err(|e| format!("Failed to open DB: {}", e))?,
            model_name: model.to_string(),
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
        let auth_token = self.resolve_auth_token()?;
        self.responses_client
            .call_responses_api(&auth_token, tools, chain, images, tracer)
            .map_err(|e| e)
    }

    fn resolve_auth_token(&self) -> Result<AuthToken, InfaError> {
        let conn_guard = self
            .conn
            .get()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let settings_repo = UserSettingsRepository::new(&*conn_guard);
        let models_repo = ModelsRepository::new(&*conn_guard);

        let settings = settings_repo
            .get_current()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let model_id = settings
            .current_model_id
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "No model selected"))?;
        let model = models_repo
            .find_by_id(model_id)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Model not found"))?;

        match model.auth_type {
            crate::domain::ModelAuthType::ApiKey => {
                let api_key = settings.openai_api_key.ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Missing API key")
                })?;
                Ok(AuthToken::ApiKey(api_key))
            }
            crate::domain::ModelAuthType::OAuth => {
                let mut access_token = settings.oauth_access_token.ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Missing OAuth token")
                })?;
                let refresh_token_val = settings.oauth_refresh_token.ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Missing OAuth token")
                })?;
                let account_id = settings.oauth_account_id;

                if settings
                    .oauth_token_expiry
                    .map(|exp| chrono::Utc::now().timestamp() >= exp)
                    .unwrap_or(true)
                {
                    let new_tokens = refresh_oauth_tokens(&refresh_token_val)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    settings_repo
                        .update_oauth_tokens(
                            &new_tokens.access_token,
                            &new_tokens.refresh_token,
                            new_tokens.expires_in,
                            new_tokens.account_id.as_deref(),
                        )
                        .map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                        })?;
                    access_token = new_tokens.access_token;
                }

                Ok(AuthToken::OAuth {
                    token: access_token,
                    account_id,
                })
            }
            crate::domain::ModelAuthType::Local => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "OpenAI model has local auth_type",
            )
            .into()),
        }
    }
}
