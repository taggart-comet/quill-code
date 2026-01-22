use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpanData {
    #[serde(rename = "agent")]
    Agent(AgentSpanData),
    #[serde(rename = "generation")]
    Generation(GenerationSpanData),
    #[serde(rename = "function")]
    Function(FunctionSpanData),
    #[serde(rename = "guardrail")]
    Guardrail(GuardrailSpanData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpanData {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoffs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationSpanData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<serde_json::Value>>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpanData {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_data: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailSpanData {
    pub name: String,
    #[serde(default)]
    pub triggered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing)]
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailResult {
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    #[serde(default = "trace_object")]
    pub object: String,
    #[serde(default = "empty_string")]
    pub id: String,
    #[serde(skip_serializing)]
    pub trace_id: String,
    #[serde(skip_serializing)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(skip_serializing)]
    pub tracing_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    #[serde(default = "span_object")]
    pub object: String,
    #[serde(default = "empty_string")]
    pub id: String,
    pub trace_id: String,
    #[serde(skip_serializing)]
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub span_data: SpanData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SpanError>,
    #[serde(skip_serializing)]
    pub tracing_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraceOrSpan {
    Trace(Trace),
    Span(Span),
}

impl Trace {
    pub fn new(name: impl Into<String>) -> Self {
        let trace_id = format!("trace_{}", Uuid::new_v4().to_string().replace("-", ""));
        let name = name.into();
        Self {
            object: trace_object(),
            id: trace_id.clone(),
            trace_id,
            name: name.clone(),
            workflow_name: Some(name),
            group_id: None,
            metadata: None,
            disabled: false,
            tracing_api_key: None,
        }
    }

    pub fn with_workflow_name(mut self, name: impl Into<String>) -> Self {
        self.workflow_name = Some(name.into());
        self
    }

    pub fn with_group_id(mut self, id: impl Into<String>) -> Self {
        self.group_id = Some(id.into());
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.tracing_api_key = Some(api_key.into());
        self
    }
}

impl Span {
    pub fn new(trace_id: impl Into<String>, span_data: SpanData) -> Self {
        let now = Utc::now();
        let span_id = format!("span_{}", Uuid::new_v4().to_string().replace("-", ""));
        Self {
            object: span_object(),
            id: span_id.clone(),
            trace_id: trace_id.into(),
            span_id,
            parent_id: None,
            started_at: now,
            ended_at: now,
            span_data,
            error: None,
            tracing_api_key: None,
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.tracing_api_key = Some(api_key.into());
        self
    }

    pub fn with_error(mut self, error: SpanError) -> Self {
        self.error = Some(error);
        self
    }

    pub fn mark_ended(&mut self) {
        self.ended_at = Utc::now();
    }
}

impl UsageData {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            input_tokens: prompt_tokens,
            output_tokens: completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

impl AgentSpanData {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            handoffs: None,
            tools: None,
            output_type: None,
        }
    }

    pub fn with_handoffs(mut self, handoffs: Vec<String>) -> Self {
        self.handoffs = Some(handoffs);
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_output_type(mut self, output_type: impl Into<String>) -> Self {
        self.output_type = Some(output_type.into());
        self
    }
}

impl GenerationSpanData {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            input: None,
            output: None,
            model: model.into(),
            model_config: None,
            usage: None,
        }
    }

    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(vec![serde_json::json!({ "content": input.into() })]);
        self
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(vec![serde_json::json!({ "content": output.into() })]);
        self
    }

    pub fn with_usage(mut self, usage: UsageData) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_model_config(mut self, config: HashMap<String, serde_json::Value>) -> Self {
        self.model_config = Some(config);
        self
    }
}

impl FunctionSpanData {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            input: None,
            output: None,
            mcp_data: None,
        }
    }

    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(input.into());
        self
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn with_mcp_data(mut self, data: HashMap<String, serde_json::Value>) -> Self {
        self.mcp_data = Some(data);
        self
    }
}

impl GuardrailSpanData {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            triggered: false,
        }
    }

    pub fn with_triggered(mut self, triggered: bool) -> Self {
        self.triggered = triggered;
        self
    }
}

impl GuardrailResult {
    pub fn passed(details: Option<String>) -> Self {
        Self { passed: true, details }
    }

    pub fn failed(details: Option<String>) -> Self {
        Self { passed: false, details }
    }
}

fn trace_object() -> String {
    "trace".to_string()
}

fn span_object() -> String {
    "trace.span".to_string()
}

fn empty_string() -> String {
    String::new()
}

fn is_false(value: &bool) -> bool {
    !*value
}
