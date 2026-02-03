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
            OpenAIClientError::Api { status, body } => {
                write!(f, "OpenAI API error (status={}, body={})", status, body)
            }
            OpenAIClientError::Deserialize { source, body } => {
                write!(
                    f,
                    "Failed to deserialize OpenAI response: {} (body={})",
                    source, body
                )
            }
            OpenAIClientError::NoText { body } => {
                write!(f, "No output_text found in OpenAI response (body={})", body)
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
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        mut tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        let max_attempts = 3;
        for attempt in 1..=max_attempts {
            match self.call_responses_api_inner(
                tools,
                chain,
                images,
                tracer.as_deref_mut(),
            ) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let error_str = e.to_string();
                    let mut should_retry = error_str.contains("error sending request")
                        || error_str.contains("connection")
                        || error_str.contains("timeout");

                    if let Some(OpenAIClientError::Api { status, .. }) =
                        e.downcast_ref::<OpenAIClientError>()
                    {
                        let status_code = status.as_u16();
                        should_retry = should_retry
                            || status.is_server_error()
                            || status_code == 429
                            || status_code == 408
                            || status_code == 409;
                    }

                    if should_retry && attempt < max_attempts {
                        let backoff_secs = 2u64.saturating_mul(attempt as u64);
                        log::warn!(
                            "OpenAI API request failed, retrying (attempt {}/{}): {}",
                            attempt,
                            max_attempts,
                            error_str
                        );
                        std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        unreachable!("retry loop exits via return");
    }

    fn call_responses_api_inner(
        &self,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        mut tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        let url = "https://api.openai.com/v1/responses";

        let request_body = build_request_dto(
            &self.model,
            images,
            tools,
            chain,
            tracer.as_deref_mut(),
        );

        // Start span with model name and add request as JSON
        if let Some(tracer) = &mut tracer {
            tracer.start_span(&self.model, openai_agents_tracing::SpanKind::Generation);

            // Convert request_body to JSON Value and set as input
            if let Ok(request_json) = serde_json::to_value(&request_body) {
                tracer.set_input_json(&self.model, request_json);
            }
        }

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()?;

        let status = response.status();
        let body = response.text()?;

        if !status.is_success() {
            if let Some(t) = tracer {
                t.end_span(&self.model);
            }
            return Err(Box::new(OpenAIClientError::Api { status, body }));
        }

        let dto = match serde_json::from_str::<ResponseDTO>(&body) {
            Ok(v) => v,
            Err(e) => {
                if let Some(t) = tracer {
                    t.end_span(&self.model);
                }
                return Err(Box::new(OpenAIClientError::Deserialize { source: e, body }));
            }
        };

        // Add response as JSON and end span
        if let Some(tracer) = &mut tracer {
            if let Ok(response_json) = serde_json::to_value(&dto) {
                tracer.set_output_json(&self.model, response_json);
            }
            tracer.end_span(&self.model);
        }

        let result = build_llm_result(dto, tools);
        if result.summary.is_empty() && result.tool_call.is_none() {
            return Err(Box::new(OpenAIClientError::NoText { body }));
        }

        Ok(result)
    }
}
