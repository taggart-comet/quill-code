use openai_agents_tracing::{
    agent_span, function_span, guardrail_span, generation_span, span_end, trace, SpanData,
    SpanKind, TraceOrSpan, TracingClient, TracingFacade, UsageData,
};

#[tokio::test]
async fn e2e_tracing_export() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set for e2e tracing test");

    let client = TracingClient::new(api_key);
    let trace = trace("E2E trace").with_workflow_name("E2E workflow");
    let trace_id = trace.trace_id.clone();

    let mut agent = agent_span(&trace_id, "E2E Agent");
    if let SpanData::Agent(ref mut data) = agent.span_data {
        data.tools = Some(vec!["search".to_string(), "summarize".to_string()]);
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

#[tokio::test]
async fn e2e_tracing_facade() {
    let api_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set for e2e tracing test");
    let mut facade = TracingFacade::new(api_key, "E2E facade trace");

    facade.start_span("agent", SpanKind::Agent);
    facade.start_span("generation", SpanKind::Generation);
    facade.end_span("agent");
    facade.end().await;
}
