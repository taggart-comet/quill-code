use super::dto::ResponseDTO;
use super::translator::{build_llm_result, build_request_dto};
use crate::infrastructure::inference::LLMInferenceResult;
use std::error::Error;

#[derive(Debug)]
pub enum OpenAIClientError {
    Api {
        status: reqwest::StatusCode,
        body: String,
    },
    Deserialize {
        source: serde_json::Error,
        body: String,
    },
    NoText {
        body: String,
    },
}

impl std::fmt::Display for OpenAIClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAIClientError::Api { status, .. } => {
                write!(f, "OpenAI API error (status={})", status)
            }
            OpenAIClientError::Deserialize { source, .. } => {
                write!(f, "Failed to deserialize OpenAI response: {}", source)
            }
            OpenAIClientError::NoText { .. } => {
                write!(f, "No output_text found in OpenAI response")
            }
        }
    }
}

impl std::error::Error for OpenAIClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OpenAIClientError::Deserialize { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct OpenAIClient {
    api_key: String,
    model: String,
    client: reqwest::blocking::Client,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn call_responses_api(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        // Try once, retry on transient errors
        match self.call_responses_api_inner(system_prompt, user_prompt, tools, chain) {
            Ok(result) => Ok(result),
            Err(e) => {
                // Check if it's a transient error worth retrying
                let error_str = e.to_string();
                if error_str.contains("error sending request")
                    || error_str.contains("connection")
                    || error_str.contains("timeout")
                {
                    log::warn!("OpenAI API request failed, retrying once: {}", error_str);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    self.call_responses_api_inner(system_prompt, user_prompt, tools, chain)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn call_responses_api_inner(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        let url = "https://api.openai.com/v1/responses";

        let request_body = build_request_dto(&self.model, system_prompt, user_prompt, tools, chain);

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()?;

        let status = response.status();
        let body = response.text()?;

        if !status.is_success() {
            return Err(Box::new(OpenAIClientError::Api { status, body }));
        }

        let dto = match serde_json::from_str::<ResponseDTO>(&body) {
            Ok(v) => v,
            Err(e) => return Err(Box::new(OpenAIClientError::Deserialize { source: e, body })),
        };

        let result = build_llm_result(dto);
        if result.summary.is_empty() && result.chosen_tool.is_none() {
            return Err(Box::new(OpenAIClientError::NoText { body }));
        }
        Ok(result)
    }
}
