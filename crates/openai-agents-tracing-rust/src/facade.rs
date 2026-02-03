use crate::client::TracingClient;
use crate::tracing::{
    agent_span, function_span, guardrail_span, generation_span, trace, trace_end,
};
use crate::types::{Span, SpanData, Trace, TraceOrSpan};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub enum SpanKind {
    Agent,
    Generation,
    Function,
    Guardrail,
}

pub struct TracingFacade {
    client: TracingClient,
    trace: Trace,
    open_spans: HashMap<String, Span>,
    ended_spans: Vec<Span>,
}

impl TracingFacade {
    pub fn new(api_key: impl Into<String>, trace_name: impl Into<String>) -> Self {
        Self {
            client: TracingClient::new(api_key),
            trace: trace(trace_name),
            open_spans: HashMap::new(),
            ended_spans: Vec::new(),
        }
    }

    pub fn start_span(&mut self, name: impl Into<String>, kind: SpanKind) {
        let name = name.into();
        let span = match kind {
            SpanKind::Agent => agent_span(&self.trace.trace_id, &name),
            SpanKind::Generation => generation_span(&self.trace.trace_id, &name),
            SpanKind::Function => function_span(&self.trace.trace_id, &name),
            SpanKind::Guardrail => guardrail_span(&self.trace.trace_id, &name),
        };

        if let Some(mut previous) = self.open_spans.insert(name, span) {
            previous.mark_ended();
            self.ended_spans.push(previous);
        }
    }

    pub fn end_span(&mut self, name: impl AsRef<str>) {
        if let Some(mut span) = self.open_spans.remove(name.as_ref()) {
            span.mark_ended();
            self.ended_spans.push(span);
        }
    }

    pub fn add_input(&mut self, name: impl AsRef<str>, input: impl Into<String>) {
        let input = input.into();
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            match span.span_data {
                SpanData::Generation(ref mut data) => {
                    data.input = Some(vec![serde_json::json!({ "content": input })]);
                }
                SpanData::Function(ref mut data) => {
                    data.input = Some(input);
                }
                _ => {}
            }
        }
    }

    pub fn add_output(&mut self, name: impl AsRef<str>, output: impl Into<String>) {
        let output = output.into();
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            match span.span_data {
                SpanData::Generation(ref mut data) => {
                    data.output = Some(vec![serde_json::json!({ "content": output })]);
                }
                SpanData::Function(ref mut data) => {
                    data.output = Some(output);
                }
                _ => {}
            }
        }
    }

    pub fn set_model_config(&mut self, name: impl AsRef<str>, config: HashMap<String, serde_json::Value>) {
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            if let SpanData::Generation(ref mut data) = span.span_data {
                data.model_config = Some(config);
            }
        }
    }

    pub fn set_usage(&mut self, name: impl AsRef<str>, input_tokens: u32, output_tokens: u32) {
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            if let SpanData::Generation(ref mut data) = span.span_data {
                data.usage = Some(crate::types::UsageData::new(input_tokens, output_tokens));
            }
        }
    }

    pub fn set_input_json(&mut self, name: impl AsRef<str>, input: serde_json::Value) {
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            if let SpanData::Generation(ref mut data) = span.span_data {
                data.input = Some(vec![input]);
            }
        }
    }

    pub fn set_output_json(&mut self, name: impl AsRef<str>, output: serde_json::Value) {
        if let Some(span) = self.open_spans.get_mut(name.as_ref()) {
            if let SpanData::Generation(ref mut data) = span.span_data {
                data.output = Some(vec![output]);
            }
        }
    }

    pub async fn end(&mut self) {
        for (_, mut span) in self.open_spans.drain() {
            span.mark_ended();
            self.ended_spans.push(span);
        }

        let trace = trace_end(self.trace.clone());
        let mut items = Vec::with_capacity(self.ended_spans.len() + 1);
        items.push(TraceOrSpan::Trace(trace));
        items.extend(self.ended_spans.drain(..).map(TraceOrSpan::Span));

        let _ = self.client.export(items).await;
    }
}
