# OpenAI Agents Tracing (Rust)

Minimal Rust implementation of the OpenAI Agents SDK tracing payloads. It provides manual APIs to create traces and spans and export them to the OpenAI Traces ingest endpoint.

## Usage

### Manual usage

```rust
use openai_agents_tracing::{
    agent_span, function_span, guardrail_span, generation_span, span_end, trace, SpanData,
    TraceOrSpan, TracingClient, UsageData,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")?;
    let client = TracingClient::new(api_key);

    let trace = trace("Example workflow");
    let trace_id = trace.trace_id.clone();

    let mut agent = agent_span(&trace_id, "Example agent");
    if let SpanData::Agent(ref mut data) = agent.span_data {
        data.tools = Some(vec!["search".to_string()]);
    }
    agent = span_end(agent);

    let mut generation = generation_span(&trace_id, "gpt-4o-mini");
    if let SpanData::Generation(ref mut data) = generation.span_data {
        data.input = Some(vec![serde_json::json!({ "content": "Hello" })]);
        data.output = Some(vec![serde_json::json!({ "content": "Hi" })]);
        data.usage = Some(UsageData::new(4, 2));
    }
    generation = span_end(generation);

    let mut function_call = function_span(&trace_id, "lookup_customer");
    if let SpanData::Function(ref mut data) = function_call.span_data {
        data.input = Some("{\"id\": 42}".to_string());
        data.output = Some("{\"status\": \"ok\"}".to_string());
    }
    function_call = span_end(function_call);

    let mut guardrail = guardrail_span(&trace_id, "content_safety");
    if let SpanData::Guardrail(ref mut data) = guardrail.span_data {
        data.triggered = false;
    }
    guardrail = span_end(guardrail);

    client
        .export(vec![
            TraceOrSpan::Trace(trace),
            TraceOrSpan::Span(agent),
            TraceOrSpan::Span(generation),
            TraceOrSpan::Span(function_call),
            TraceOrSpan::Span(guardrail),
        ])
        .await?;

    Ok(())
}
```

### Facade usage

```rust
use openai_agents_tracing::{SpanKind, TracingFacade};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    let mut facade = TracingFacade::new(api_key, "Example workflow");

    facade.start_span("agent", SpanKind::Agent);
    facade.end_span("agent");

    facade.start_span("generation", SpanKind::Generation);
    facade.start_span("function_call", SpanKind::Function);
    facade.end_span("function_call");

    facade.end().await;
}
```

## E2E Test

The test sends real traces to OpenAI. It requires `OPENAI_API_KEY` to be set, otherwise it fails.

```bash
OPENAI_API_KEY=sk-... cargo test --test e2e_tracing
```
