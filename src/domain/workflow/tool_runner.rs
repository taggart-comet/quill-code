use crate::domain::permissions::PermissionChecker;
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use crate::domain::tools::ToolResult;
use openai_agents_tracing::TracingFacade;
use std::sync::Arc;

pub struct ToolRunner {
    permission_checker: Arc<PermissionChecker>,
}

impl ToolRunner {
    pub fn new(permission_checker: Arc<PermissionChecker>) -> Self {
        Self {
            permission_checker,
        }
    }

    pub fn run(
        &self,
        request: &mut dyn Request,
        tool: &dyn Tool,
        tracer: Option<&mut TracingFacade>,
    ) -> ToolResult {
        match tracer {
            Some(tracer) => {
                tracer.start_span(tool.name(), openai_agents_tracing::SpanKind::Function);
                tracer.add_input(tool.name(), tool.get_input());

                let result = self._run(tool, request);

                tracer.add_output(tool.name(), result.output_string());
                tracer.end_span(tool.name());
                result
            }
            None => {
                self._run(tool, request)
            },
        }
    }

    fn _run(&self, tool: &dyn Tool, request: &mut dyn Request) -> ToolResult {
        match self
            .permission_checker
            .check(tool, request, request.project_id())
        {
            Ok(true) => tool.work(request),
            Ok(false) => ToolResult::error(
                tool.name().to_string(),
                String::new(),
                "Permission denied".to_string(),
            ),
            Err(err) => ToolResult::error(
                tool.name().to_string(),
                String::new(),
                format!("Permission check error: {}", err),
            ),
        }
    }
}

