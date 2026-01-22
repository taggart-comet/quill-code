pub mod client;
pub mod facade;
pub mod tracing;
pub mod types;

pub use client::TracingClient;
pub use facade::{SpanKind, TracingFacade};
pub use tracing::{
    agent_span, function_span, guardrail_span, generation_span, span_end, trace, trace_end,
};
pub use types::{
    AgentSpanData, FunctionSpanData, GenerationSpanData, GuardrailResult, GuardrailSpanData, Span,
    SpanData, SpanError, Trace, TraceOrSpan, UsageData,
};
