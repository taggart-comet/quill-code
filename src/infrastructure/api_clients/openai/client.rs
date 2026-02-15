use super::dto::ResponseDTO;
use super::translator::{build_llm_result, build_request_dto};
use crate::infrastructure::inference::LLMInferenceResult;
use serde_json::Value;
use std::error::Error;
use std::io::Read;

#[derive(Clone)]
pub enum AuthToken {
    ApiKey(String),
    OAuth {
        token: String,
        account_id: Option<String>,
    },
}

#[derive(Debug)]
pub enum OpenAIClientError {
    Api {
        status: reqwest::StatusCode,
        body: String,
        request_body: String,
    },
    Deserialize {
        source: serde_json::Error,
        body: String,
    },
    NoText {
        body: String,
    },
    BodyRead {
        source: String,
        status: reqwest::StatusCode,
        content_encoding: Option<String>,
        content_type: Option<String>,
        content_length: Option<u64>,
        transfer_encoding: Option<String>,
        response_headers: String,
        partial_body: Option<String>,
    },
    AuthExpired,
}

impl std::fmt::Display for OpenAIClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAIClientError::Api {
                status,
                body,
                request_body,
            } => {
                write!(
                    f,
                    "OpenAI API error (status={}, body={}, request_body={})",
                    status, body, request_body
                )
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
            OpenAIClientError::BodyRead {
                source,
                status,
                content_encoding,
                content_type,
                content_length,
                transfer_encoding,
                response_headers,
                partial_body,
            } => {
                write!(
                    f,
                    "Failed to read SSE response body (status={}, content-encoding={}, content-type={}, content-length={}, transfer-encoding={}, headers={}, partial-body={}, error={})",
                    status,
                    content_encoding.as_deref().unwrap_or("<missing>"),
                    content_type.as_deref().unwrap_or("<missing>"),
                    content_length
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "<missing>".to_string()),
                    transfer_encoding.as_deref().unwrap_or("<missing>"),
                    response_headers,
                    partial_body.as_deref().unwrap_or("<empty>"),
                    source
                )
            }
            OpenAIClientError::AuthExpired => {
                write!(f, "OAuth token expired - please re-authenticate")
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
    model: String,
    client: reqwest::blocking::Client,
}

impl OpenAIClient {
    pub fn new(model: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .build()
            .expect("Failed to build OpenAI HTTP client");
        Self { model, client }
    }

    pub fn call_responses_api(
        &self,
        auth_token: &AuthToken,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        mut tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        let max_attempts = 3;
        let mut notext_retried = false;
        for attempt in 1..=max_attempts {
            match self.call_responses_api_inner(
                auth_token,
                tools,
                chain,
                images,
                tracer.as_deref_mut(),
            ) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if e.downcast_ref::<OpenAIClientError>()
                        .map(|err| matches!(err, OpenAIClientError::NoText { .. }))
                        .unwrap_or(false)
                    {
                        if !notext_retried {
                            notext_retried = true;
                            log::warn!(
                                "OpenAI returned empty output_text, retrying once (attempt {}/{}).",
                                attempt,
                                max_attempts
                            );
                            continue;
                        }

                        let message = "System Message: model malfunction, return empty text";
                        return Ok(LLMInferenceResult {
                            summary: message.to_string(),
                            raw_output: message.to_string(),
                            tool_call: None,
                        });
                    }
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
        auth_token: &AuthToken,
        tools: &[&dyn crate::domain::tools::Tool],
        chain: &crate::domain::workflow::Chain,
        images: &[String],
        mut tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, Box<dyn Error + Send + Sync>> {
        // Determine API URL based on auth type
        let url = match auth_token {
            AuthToken::OAuth { .. } => "https://chatgpt.com/backend-api/codex/responses",
            AuthToken::ApiKey(_) => "https://api.openai.com/v1/responses",
        };

        let allow_system_messages = !matches!(auth_token, AuthToken::OAuth { .. });
        let request_body = build_request_dto(
            &self.model,
            images,
            tools,
            chain,
            allow_system_messages,
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

        // Build request with appropriate headers
        let mut request_builder = self.client.post(url).json(&request_body);

        // Add authentication header
        match auth_token {
            AuthToken::ApiKey(key) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", key));
            }
            AuthToken::OAuth { token, account_id } => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", token));
                if let Some(acc_id) = account_id {
                    request_builder = request_builder.header("ChatGPT-Account-Id", acc_id);
                }
            }
        }

        let mut response = request_builder
            .header("Accept", "text/event-stream")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()?;

        let status = response.status();
        let content_encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let content_length = response.content_length();
        let transfer_encoding = response
            .headers()
            .get(reqwest::header::TRANSFER_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let response_headers = Self::format_headers_excerpt(response.headers());

        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|e| format!("(failed to read body: {})", e));
            if let Some(t) = tracer {
                t.end_span(&self.model);
            }

            // Check for OAuth expiration
            if status.as_u16() == 401 {
                return Err(Box::new(OpenAIClientError::AuthExpired));
            }

            let request_body_str = serde_json::to_string(&request_body)
                .unwrap_or_else(|e| format!("(failed to serialize request: {})", e));
            return Err(Box::new(OpenAIClientError::Api {
                status,
                body,
                request_body: Self::format_body_excerpt(&request_body_str, 12000),
            }));
        }

        // Read SSE stream as raw bytes to avoid encoding issues with chunked transfer
        let mut body_bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut read_error: Option<String> = None;
        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_len) => body_bytes.extend_from_slice(&buffer[..read_len]),
                Err(e) => {
                    read_error = Some(e.to_string());
                    break;
                }
            }
        }
        let body = String::from_utf8_lossy(&body_bytes).into_owned();
        log::debug!("SSE response length: {} bytes", body.len());
        log::trace!("SSE response body: {}", &body[..body.len().min(2000)]);

        let response_id = Self::extract_response_id_from_sse(&body);
        let allow_retrieve = matches!(auth_token, AuthToken::ApiKey(_));
        let mut retrieved_meta: Option<(Option<String>, String)> = None;
        let dto = match Self::parse_sse_response(&body) {
            Ok(v) => v,
            Err(e) => {
                if !allow_retrieve {
                    if let Some(t) = tracer {
                        t.end_span(&self.model);
                    }
                    if !body.contains("response.completed") {
                        return Err(Box::new(OpenAIClientError::NoText {
                            body: format!(
                                "empty response from SSE stream; sse_body={}",
                                Self::format_body_excerpt(&body, 4000)
                            ),
                        }));
                    }
                    return Err(e);
                }
                if let Some(response_id) = response_id.as_deref() {
                    match self.fetch_response_with_retry(auth_token, response_id, 4, 750) {
                        Ok((fallback, status, raw_body)) => {
                            retrieved_meta = Some((status, raw_body));
                            fallback
                        }
                        Err(fetch_err) => {
                            if let Some(t) = tracer {
                                t.end_span(&self.model);
                            }
                            return Err(fetch_err);
                        }
                    }
                } else if let Some(error_message) = read_error {
                    let partial_body = if body.is_empty() {
                        None
                    } else {
                        Some(Self::format_body_excerpt(&body, 4000))
                    };
                    if let Some(t) = tracer {
                        t.end_span(&self.model);
                    }
                    return Err(Box::new(OpenAIClientError::BodyRead {
                        source: error_message,
                        status,
                        content_encoding: content_encoding.clone(),
                        content_type: content_type.clone(),
                        content_length,
                        transfer_encoding: transfer_encoding.clone(),
                        response_headers: response_headers.clone(),
                        partial_body,
                    }));
                } else {
                    if let Some(t) = tracer {
                        t.end_span(&self.model);
                    }
                    return Err(e);
                }
            }
        };

        if let Some(error_message) = read_error {
            log::warn!(
                "SSE stream ended with read error but response.completed was parsed: {}",
                error_message
            );
        }

        // Add response as JSON and end span
        if let Some(tracer) = &mut tracer {
            if let Ok(response_json) = serde_json::to_value(&dto) {
                tracer.set_output_json(&self.model, response_json);
            }
            tracer.end_span(&self.model);
        }

        let mut result = build_llm_result(dto, tools);
        if result.summary.is_empty() && result.tool_call.is_none() {
            if allow_retrieve {
                if let Some(response_id) = response_id.as_deref() {
                    if let Ok((fallback, status, raw_body)) =
                        self.fetch_response_with_retry(auth_token, response_id, 4, 750)
                    {
                        retrieved_meta = Some((status, raw_body));
                        result = build_llm_result(fallback, tools);
                    }
                }
            }
        }
        if result.summary.is_empty() && result.tool_call.is_none() {
            if !body.contains("response.completed") {
                let retrieve_info = retrieved_meta.as_ref().map(|(status, raw_body)| {
                    format!(
                        "retrieve_status={}; retrieve_body={}",
                        status.as_deref().unwrap_or("<missing>"),
                        Self::format_body_excerpt(raw_body, 4000)
                    )
                });
                return Err(Box::new(OpenAIClientError::BodyRead {
                    source: match retrieve_info {
                        Some(info) => {
                            format!("SSE stream ended before response.completed; {}", info)
                        }
                        None => "SSE stream ended before response.completed".to_string(),
                    },
                    status,
                    content_encoding: content_encoding.clone(),
                    content_type: content_type.clone(),
                    content_length,
                    transfer_encoding: transfer_encoding.clone(),
                    response_headers: response_headers.clone(),
                    partial_body: Some(Self::format_body_excerpt(&body, 4000)),
                }));
            }
            return Err(Box::new(OpenAIClientError::NoText {
                body: {
                    let mut message = format!(
                        "empty response from SSE stream; sse_body={}",
                        Self::format_body_excerpt(&body, 4000)
                    );
                    if let Some((status, raw_body)) = retrieved_meta.as_ref() {
                        message.push_str("; ");
                        message.push_str(&format!(
                            "retrieve_status={}; retrieve_body={}",
                            status.as_deref().unwrap_or("<missing>"),
                            Self::format_body_excerpt(raw_body, 4000)
                        ));
                    }
                    message
                },
            }));
        }

        Ok(result)
    }

    /// Parse an SSE response body, extracting the ResponseDTO from the `response.completed` event.
    ///
    /// SSE format from OpenAI Responses API:
    /// ```text
    /// event: response.created
    /// data: {"type":"response.created","response":{...}}
    ///
    /// event: response.output_text.delta
    /// data: {"type":"response.output_text.delta","delta":"Hello",...}
    ///
    /// event: response.completed
    /// data: {"type":"response.completed","response":{"output":[...],...}}
    /// ```
    fn parse_sse_response(body: &str) -> Result<ResponseDTO, Box<dyn Error + Send + Sync>> {
        let mut current_event: Option<String> = None;
        let mut completed_data: Option<String> = None;

        for line in body.lines() {
            if let Some(event_name) = line.strip_prefix("event:") {
                current_event = Some(event_name.trim().to_string());
            } else if let Some(data_payload) = line.strip_prefix("data:") {
                let data = data_payload.trim_start();
                if current_event.as_deref() == Some("response.completed") {
                    completed_data = Some(data.to_string());
                    break;
                }
            }
            // Empty lines are event boundaries in SSE — reset current_event
            if line.is_empty() {
                current_event = None;
            }
        }

        let data = match completed_data {
            Some(d) => d,
            None => {
                // Fallback: maybe the body is plain JSON (non-SSE), try parsing directly
                log::warn!(
                    "No response.completed event found in SSE stream, trying direct JSON parse"
                );
                return serde_json::from_str::<ResponseDTO>(body.trim()).map_err(|e| {
                    log::error!(
                        "Direct JSON parse also failed. Body prefix: {}",
                        &body[..body.len().min(500)]
                    );
                    Box::new(OpenAIClientError::Deserialize {
                        source: e,
                        body: format!("sse_body={}", Self::format_body_excerpt(body, 4000)),
                    }) as Box<dyn Error + Send + Sync>
                });
            }
        };

        // The response.completed data has shape: {"type":"response.completed","response":{...}}
        let wrapper: serde_json::Value = serde_json::from_str(&data).map_err(|e| {
            log::error!(
                "Failed to parse response.completed JSON: {}",
                &data[..data.len().min(500)]
            );
            Box::new(OpenAIClientError::Deserialize {
                source: e,
                body: format!(
                    "response.completed data={}; sse_body={}",
                    Self::format_body_excerpt(&data, 1000),
                    Self::format_body_excerpt(body, 4000)
                ),
            }) as Box<dyn Error + Send + Sync>
        })?;

        // Extract the "response" field which contains the actual response object
        let response_obj = wrapper.get("response").unwrap_or(&wrapper);

        serde_json::from_value::<ResponseDTO>(response_obj.clone()).map_err(|e| {
            log::error!(
                "Failed to deserialize ResponseDTO from: {}",
                &response_obj.to_string()[..response_obj.to_string().len().min(500)]
            );
            Box::new(OpenAIClientError::Deserialize {
                source: e,
                body: format!(
                    "response_obj={}; sse_body={}",
                    Self::format_body_excerpt(&response_obj.to_string(), 1000),
                    Self::format_body_excerpt(body, 4000)
                ),
            }) as Box<dyn Error + Send + Sync>
        })
    }

    fn extract_response_id_from_sse(body: &str) -> Option<String> {
        let mut current_event: Option<String> = None;
        for line in body.lines() {
            if let Some(event_name) = line.strip_prefix("event:") {
                current_event = Some(event_name.trim().to_string());
            } else if let Some(data_payload) = line.strip_prefix("data:") {
                let data = data_payload.trim_start();
                let is_created = current_event
                    .as_deref()
                    .map(|name| name == "response.created")
                    .unwrap_or(false);
                if is_created {
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(id) = json
                            .get("response")
                            .and_then(|r| r.get("id"))
                            .and_then(|v| v.as_str())
                        {
                            return Some(id.to_string());
                        }
                        if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                            return Some(id.to_string());
                        }
                    }
                }
            }
            if line.is_empty() {
                current_event = None;
            }
        }
        None
    }

    fn parse_response_json(body: &str) -> Result<ResponseDTO, Box<dyn Error + Send + Sync>> {
        if let Ok(dto) = serde_json::from_str::<ResponseDTO>(body.trim()) {
            return Ok(dto);
        }

        let wrapper: Value = serde_json::from_str(body.trim()).map_err(|e| {
            Box::new(OpenAIClientError::Deserialize {
                source: e,
                body: Self::format_body_excerpt(body, 4000),
            }) as Box<dyn Error + Send + Sync>
        })?;

        let response_obj = wrapper.get("response").unwrap_or(&wrapper);
        serde_json::from_value::<ResponseDTO>(response_obj.clone()).map_err(|e| {
            Box::new(OpenAIClientError::Deserialize {
                source: e,
                body: response_obj.to_string(),
            }) as Box<dyn Error + Send + Sync>
        })
    }

    fn fetch_response_with_retry(
        &self,
        auth_token: &AuthToken,
        response_id: &str,
        max_attempts: usize,
        delay_ms: u64,
    ) -> Result<(ResponseDTO, Option<String>, String), Box<dyn Error + Send + Sync>> {
        let mut last: Option<(ResponseDTO, Option<String>, String)> = None;
        for attempt in 1..=max_attempts {
            let fetched = self.fetch_response_with_meta(auth_token, response_id)?;
            let status = fetched.1.clone().unwrap_or_default();
            last = Some(fetched);
            if status == "completed" || status == "failed" {
                return Ok(last.unwrap());
            }
            if attempt < max_attempts {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }

        Ok(last.unwrap())
    }

    fn fetch_response_with_meta(
        &self,
        auth_token: &AuthToken,
        response_id: &str,
    ) -> Result<(ResponseDTO, Option<String>, String), Box<dyn Error + Send + Sync>> {
        let url = match auth_token {
            AuthToken::OAuth { .. } => format!(
                "https://chatgpt.com/backend-api/codex/responses/{}",
                response_id
            ),
            AuthToken::ApiKey(_) => format!("https://api.openai.com/v1/responses/{}", response_id),
        };

        let mut request_builder = self.client.get(url).header("Accept", "application/json");
        match auth_token {
            AuthToken::ApiKey(key) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", key));
            }
            AuthToken::OAuth { token, account_id } => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", token));
                if let Some(acc_id) = account_id {
                    request_builder = request_builder.header("ChatGPT-Account-Id", acc_id);
                }
            }
        }

        let response = request_builder.send()?;
        let status = response.status();
        let response_url = response.url().to_string();
        let body = response
            .text()
            .unwrap_or_else(|e| format!("(failed to read body: {})", e));

        if !status.is_success() {
            return Err(Box::new(OpenAIClientError::Api {
                status,
                body,
                request_body: format!("GET {}", response_url),
            }));
        }

        let wrapper: Value = serde_json::from_str(body.trim()).map_err(|e| {
            Box::new(OpenAIClientError::Deserialize {
                source: e,
                body: Self::format_body_excerpt(&body, 4000),
            }) as Box<dyn Error + Send + Sync>
        })?;
        let status_value = wrapper
            .get("status")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let dto = Self::parse_response_json(&body)?;

        Ok((dto, status_value, body))
    }

    fn format_body_excerpt(body: &str, max_len: usize) -> String {
        let trimmed = body.trim();
        let total = trimmed.chars().count();
        if total <= max_len {
            trimmed.to_string()
        } else {
            let excerpt: String = trimmed.chars().take(max_len).collect();
            format!(
                "{}...[truncated {} chars]",
                excerpt,
                total.saturating_sub(max_len)
            )
        }
    }

    fn format_headers_excerpt(headers: &reqwest::header::HeaderMap) -> String {
        let mut rendered = String::new();
        for (name, value) in headers.iter() {
            if !rendered.is_empty() {
                rendered.push_str(", ");
            }
            let value_str = value.to_str().unwrap_or("<non-utf8>");
            rendered.push_str(name.as_str());
            rendered.push('=');
            rendered.push_str(value_str);
            if rendered.len() > 2000 {
                rendered.push_str("...[truncated]");
                break;
            }
        }
        if rendered.is_empty() {
            "<missing>".to_string()
        } else {
            rendered
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sse_text_response() {
        let sse_body = "\
event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"output\":[]}}\n\
\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello, world!\"}]}]}}\n\
\n";

        let dto = OpenAIClient::parse_sse_response(sse_body).expect("should parse SSE");
        let (summary, tool_call) = dto.extract_parts();
        assert_eq!(summary, "Hello, world!");
        assert!(tool_call.is_none());
    }

    #[test]
    fn parse_sse_function_call_response() {
        let sse_body = "\
event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"output\":[]}}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"output\":[{\"type\":\"function_call\",\"name\":\"shell_exec\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\",\"call_id\":\"call_abc\"}]}}\n\
\n";

        let dto = OpenAIClient::parse_sse_response(sse_body).expect("should parse SSE");
        let (summary, tool_call) = dto.extract_parts();
        assert!(summary.is_empty());
        let call = tool_call.expect("should have tool call");
        assert_eq!(call.name, "shell_exec");
        assert_eq!(call.call_id, "call_abc");
    }

    #[test]
    fn parse_sse_mixed_text_and_function_call() {
        let sse_body = "\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Let me check.\"}]},{\"type\":\"function_call\",\"name\":\"read_file\",\"arguments\":\"{}\",\"call_id\":\"call_xyz\"}]}}\n\
\n";

        let dto = OpenAIClient::parse_sse_response(sse_body).expect("should parse SSE");
        let (summary, tool_call) = dto.extract_parts();
        assert_eq!(summary, "Let me check.");
        assert!(tool_call.is_some());
    }

    #[test]
    fn parse_sse_no_completed_event_falls_back_to_direct_json() {
        // If body is plain JSON (not SSE), should still work
        let json_body = "{\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Direct JSON\"}]}]}";

        let dto = OpenAIClient::parse_sse_response(json_body).expect("should parse direct JSON");
        let (summary, _) = dto.extract_parts();
        assert_eq!(summary, "Direct JSON");
    }

    #[test]
    fn parse_sse_empty_body_errors() {
        let result = OpenAIClient::parse_sse_response("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_sse_ignores_earlier_events() {
        // Ensure we pick up data from response.completed, not from earlier events
        let sse_body = "\
event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\",\"output\":[]}}\n\
\n\
event: response.in_progress\n\
data: {\"type\":\"response.in_progress\"}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Final answer\"}]}]}}\n\
\n";

        let dto = OpenAIClient::parse_sse_response(sse_body).expect("should parse");
        let (summary, _) = dto.extract_parts();
        assert_eq!(summary, "Final answer");
    }
}
