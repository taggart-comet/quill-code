use crate::types::TraceOrSpan;
use anyhow::{anyhow, Result};
use reqwest::Client;
use std::collections::HashMap;

pub struct TracingClient {
    client: Client,
    api_key: String,
    endpoint: String,
    organization: Option<String>,
    project: Option<String>,
}

impl TracingClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            endpoint: "https://api.openai.com/v1/traces/ingest".to_string(),
            organization: None,
            project: None,
        }
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    pub async fn export(&self, items: Vec<TraceOrSpan>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let mut grouped: HashMap<Option<String>, Vec<TraceOrSpan>> = HashMap::new();
        for item in items {
            let key = match &item {
                TraceOrSpan::Trace(trace) => trace.tracing_api_key.clone(),
                TraceOrSpan::Span(span) => span.tracing_api_key.clone(),
            };
            grouped.entry(key).or_default().push(item);
        }

        for (item_key, group) in grouped {
            let api_key = item_key.unwrap_or_else(|| self.api_key.clone());
            if api_key.is_empty() {
                return Err(anyhow!("OPENAI_API_KEY is not set"));
            }

            let payload = serde_json::json!({ "data": group });
            let mut request = self.client.post(&self.endpoint);
            request = request.header("Authorization", format!("Bearer {}", api_key));
            request = request.header("Content-Type", "application/json");
            request = request.header("OpenAI-Beta", "traces=v1");

            if let Some(ref org) = self.organization {
                request = request.header("OpenAI-Organization", org);
            }
            if let Some(ref project) = self.project {
                request = request.header("OpenAI-Project", project);
            }

            let response = request.json(&payload).send().await?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow!("Tracing export failed: {} {}", status, body));
            }
        }

        Ok(())
    }
}
