use crate::domain::permissions::PermissionChecker;
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use crate::domain::tools::ToolResult;
use crate::domain::workflow::toolset::Toolset;
use crate::infrastructure::event_bus::{AgentToUiEvent, StepPhase};
use crate::infrastructure::inference::ToolCall;
use crossbeam_channel::Sender;
use openai_agents_tracing::TracingFacade;
use std::sync::Arc;

pub struct ToolRunner {
    permission_checker: Arc<PermissionChecker>,
    event_sender: Sender<AgentToUiEvent>,
}

impl ToolRunner {
    pub fn new(
        permission_checker: Arc<PermissionChecker>,
        event_sender: Sender<AgentToUiEvent>,
    ) -> Self {
        Self {
            permission_checker,
            event_sender,
        }
    }

    pub fn run(
        &self,
        request: &mut dyn Request,
        tool_call: ToolCall,
        toolset: &dyn Toolset,
        tracer: Option<&mut TracingFacade>,
    ) -> ToolResult {
        let tool = match toolset.prepare_tool(&tool_call) {
            Ok(t) => t,
            Err(e) => {
                let error_msg = format!("Failed to prepare tool '{}': {}", tool_call.name, e);
                log::error!("{}", error_msg);
                return ToolResult::error(
                    tool_call.name.clone(),
                    tool_call.arguments.clone(),
                    error_msg,
                    tool_call.call_id.clone(),
                );
            }
        };

        match tracer {
            Some(tracer) => {
                tracer.start_span(tool.name(), openai_agents_tracing::SpanKind::Function);
                tracer.add_input(tool.name(), tool.get_input());

                let result = self._run(tool, request);

                tracer.add_output(tool.name(), result.output_string());
                tracer.end_span(tool.name());
                result
            }
            None => self._run(tool, request),
        }
    }

    fn _run(&self, tool: &dyn Tool, request: &mut dyn Request) -> ToolResult {
        let mut result = match self
            .permission_checker
            .check(tool, request, request.project_id())
        {
            Ok(true) => {
                self.emit_progress(tool, request);
                tool.work(request)
            }
            Ok(false) => ToolResult::error(
                tool.name().to_string(),
                String::new(),
                "Permission denied".to_string(),
                String::new(),
            ),
            Err(err) => ToolResult::error(
                tool.name().to_string(),
                String::new(),
                format!("Permission check error: {}", err),
                String::new(),
            ),
        };

        if let Some(limit) = tool.get_output_budget() {
            result.apply_output_budget(limit);
        }

        result
    }

    fn emit_progress(&self, tool: &dyn Tool, request: &dyn Request) {
        let _ = self.event_sender.send(AgentToUiEvent::ProgressEvent {
            step_name: tool.name().to_string(),
            phase: StepPhase::Before,
            summary: tool.get_progress_message(request),
        });
    }
}
