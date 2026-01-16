use super::dto::{ResponseDTO, RequestDTO};
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

    pub fn call_responses_api(&self, prompt: &str) -> Result<String, Box<dyn Error>> {
        // Try once, retry on transient errors
        match self.call_responses_api_inner(prompt) {
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
                    self.call_responses_api_inner(prompt)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn call_responses_api_inner(&self, prompt: &str) -> Result<String, Box<dyn Error>> {
        let url = "https://api.openai.com/v1/responses";

        let request_body = RequestDTO {
            model: self.model.clone(),
            instructions: String::new(),
            input: vec![InputDto {
                content: prompt.to_string(),
            }],
            tools: vec![],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: true,
            reasoning: ReasoningConfig {
                summary: "auto".to_string(),
            },
            store: false,
            stream: false,
        };

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

        let result = dto.extract_text();
        if result.is_empty() {
            return Err(Box::new(OpenAIClientError::NoText { body }));
        }
        Ok(result)
    }
}
