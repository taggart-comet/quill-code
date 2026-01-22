use super::{
    chain::Chain, step::ChainStep, step::StepType, tool_runner::ToolRunner, toolset::Toolset,
    CancellationToken, Error,
};
use crate::domain::bt::GeneralTree;
use crate::domain::{prompting, user_settings};
use crate::domain::session::Request;
use crate::domain::workflow::AllToolset;
use crate::domain::ModelType;
use crate::infrastructure::inference::InferenceEngine;
use tokio::runtime::{Builder, Handle};

use std::sync::Arc;
use openai_agents_tracing::{SpanKind, TracingFacade};

/// Main workflow orchestrator that runs LLM-driven coding tasks
/// Implements an eternal agent loop that:
/// 1. Asks LLM to choose the next tool
/// 2. Checks permissions for the tool execution
/// 3. Executes that tool (if permitted)
/// 4. Stores the result in the chain
/// 5. Repeats until 128 iterations reached
pub struct Workflow {
    toolset: Box<dyn Toolset>,
    engine: Arc<dyn InferenceEngine>,
    chain: Chain,
    tool_runner: ToolRunner,
    tracer: Option<TracingFacade>,
}

impl Workflow {
    /// Create a new workflow with permission checking enabled
    pub fn new(
        engine: Arc<dyn InferenceEngine>,
        permission_checker: Arc<crate::domain::permissions::PermissionChecker>,
    ) -> Result<Self, String> {
        Ok(Self {
            toolset: Box::new(AllToolset::new()),
            engine,
            chain: Chain::new(),
            tool_runner: ToolRunner::new(permission_checker),
            tracer: None,
        })
    }

    pub fn get_chain(&self) -> &Chain {
        &self.chain
    }

    pub fn run(
        &mut self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
        max_tool_calls: usize,
        user_prompt_override: Option<String>,
    ) -> Result<(), Error> {
        self._init_tracer(request, "Code Generation Agentic Workflow");

        self._reset_chain();
        let result = self._run(request, cancel, max_tool_calls, user_prompt_override);
        self._end_tracing();
        result
    }

    pub fn run_using_bt(
        &mut self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
    ) -> Result<Chain, Error> {

        self._init_tracer(request, "[Behavior Tree] Code Generation Agentic Workflow");

        self._reset_chain();
        let tree = GeneralTree::new();
        let mut current_step = Some(tree.head());

        while let Some(step) = current_step {
            let step_user_prompt =
                prompting::get_bt_tree_step_prompt(self.engine.get_type(), step, request);

            self.toolset = step.toolset().build();

            self._run(
                request,
                cancel,
                step.max_tools_calls() as usize,
                Some(step_user_prompt.clone()),
            )?;
            self.chain.steps.push(ChainStep {
                step_type: StepType::BehaviorTreeStepPassed.as_str().to_string(),
                summary: self.chain.final_message().unwrap_or("").to_string(),
                context_payload: String::new(),
                input_payload: step_user_prompt.to_string(),
                tool_name: None,
                tool_output: None,
                is_successful: Some(true),
                file_changes: None,
            });
            if self.chain.is_failed {
                self._end_tracing();
                return Ok(self.chain.clone());
            }

            current_step = step.next_step();
        }

        self._end_tracing();
        Ok(self.chain.clone())
    }

    fn _run(
        &mut self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
        max_tool_calls: usize,
        user_prompt_override: Option<String>,
    ) -> Result<(), Error> {
        let mut web_search_calls = 0usize;

        // Eternal agent loop
        for _iteration in 1..=max_tool_calls {
            // Check for cancellation at the start of each iteration
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                self.chain.add_interruption();
                return Ok(());
            }

            let system_prompt = prompting::get_system_prompt(self.engine.get_type());
            let base_user_prompt = prompting::get_user_prompt(self.engine.get_type(), request);
            let user_prompt = user_prompt_override
                .as_deref()
                .unwrap_or(&base_user_prompt)
                .to_string();

            self._trace_llm_start(user_prompt.clone());

            // Ask LLM to choose next tool
            let llm_output = match self.engine.generate(
                &system_prompt,
                &user_prompt,
                1024,
                &self.toolset.tool_refs(),
                &self.chain,
            ) {
                Ok(output) => output,
                Err(err) => {
                    return Err(Error::Inference(err.to_string()));
                }
            };

            self._trace_llm_end(llm_output.raw_output.clone());

            // Check for cancellation after inference
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                self.chain.add_interruption();
                return Ok(());
            }

            if llm_output.chosen_tool.is_none() {
                let final_message = llm_output.summary;
                self.chain.set_final_message(final_message.clone());
                request.set_final_message(final_message);
                self._end_tracing();
                return Ok(());
            }

            let chosen_tool = llm_output.chosen_tool.unwrap();

            let is_web_search = chosen_tool.name() == "web_search";
            let tool_result = if is_web_search && web_search_calls >= 2 {
                crate::domain::tools::ToolResult::error(
                    chosen_tool.name().to_string(),
                    String::new(),
                    "Web search call limit exceeded (2 per request)".to_string(),
                )
            } else {
                if is_web_search {
                    web_search_calls += 1;
                }

                self.tool_runner
                    .run(request, chosen_tool.as_ref(), self.tracer.as_mut())
            };

            // Add step to chain
            self.chain.add_step(tool_result);
        }

        if max_tool_calls > 0 {
            log::warn!("Workflow reached maximum iterations ({})", max_tool_calls);
        }
        Ok(())
    }

    fn _init_tracer(&mut self, request: &mut dyn Request, trace_name: &str) {
        if self.tracer.is_some() {
            return;
        }
        let settings = request.user_settings().unwrap();
        if !settings.openai_tracing_enabled() {
            return;
        }
        let api_key = settings.openai_api_key().unwrap();
        if api_key.is_empty() {
            return;
        }
        self.tracer = Some(TracingFacade::new(api_key, trace_name.to_string()));
    }

    fn _end_tracing(&mut self) {
        // `TracingFacade::end()` is async; we block here once at shutdown.
        if let Some(mut tracer) = self.tracer.take() {
            // If we're already inside a Tokio runtime, use it; otherwise create a minimal current-thread runtime.
            if let Ok(handle) = Handle::try_current() {
                let _ = handle.block_on(tracer.end());
            } else {
                let rt = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime");
                let _ = rt.block_on(tracer.end());
            }
        }
    }

    fn _reset_chain(&mut self) {
        self.chain = Chain::new();
    }

    fn _trace_llm_start(&mut self, prompt: String) {
        if let Some(tracer) = &mut self.tracer {
            tracer.start_span("LLM generation", SpanKind::Generation);
            tracer.add_input("LLM generation", prompt);
        }
    }

    fn _trace_llm_end(&mut self, output: String) {
        if let Some(tracer) = &mut self.tracer {
            tracer.add_output("LLM generation", output);
            tracer.end_span("LLM generation");
        }
    }
}
