use crate::types::{
    AgentSpanData, GenerationSpanData, GuardrailSpanData, Span, SpanData, Trace,
    FunctionSpanData,
};

pub fn trace(name: impl Into<String>) -> Trace {
    Trace::new(name)
}

pub fn trace_end(trace: Trace) -> Trace {
    trace
}

pub fn span_end(mut span: Span) -> Span {
    span.mark_ended();
    span
}

pub fn agent_span(trace_id: impl Into<String>, agent_name: impl Into<String>) -> Span {
    let data = AgentSpanData::new(agent_name);
    Span::new(trace_id, SpanData::Agent(data))
}

pub fn generation_span(trace_id: impl Into<String>, model: impl Into<String>) -> Span {
    let data = GenerationSpanData::new(model);
    Span::new(trace_id, SpanData::Generation(data))
}

pub fn function_span(trace_id: impl Into<String>, function_name: impl Into<String>) -> Span {
    let data = FunctionSpanData::new(function_name);
    Span::new(trace_id, SpanData::Function(data))
}

pub fn guardrail_span(trace_id: impl Into<String>, guardrail_name: impl Into<String>) -> Span {
    let data = GuardrailSpanData::new(guardrail_name);
    Span::new(trace_id, SpanData::Guardrail(data))
}
