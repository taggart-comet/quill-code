use super::{
    chain::Chain, step::ChainStep, step::StepType, tool_runner::ToolRunner, toolset::Toolset,
    CancellationToken, Error,
};
use crate::domain::bt::GeneralTree;
use crate::domain::prompting;
use crate::domain::session::Request;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{AgentToUiEvent, StepPhase};
use crate::infrastructure::inference::InferenceEngine;
use tokio::runtime::{Builder, Handle};
use crate::domain::workflow::toolset::NoneToolset;
use crate::domain::workflow::toolset::ToolsetType;
use openai_agents_tracing::{SpanKind, TracingFacade};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use crossbeam_channel::Sender;

/// Main workflow orchestrator that runs LLM-driven coding tasks
/// Implements an eternal agent loop that:
/// 1. Asks LLM to choose the next tool
/// 2. Checks permissions for the tool execution
/// 3. Executes that tool (if permitted)
/// 4. Stores the result in the chain
/// 5. Repeats until 128 iterations reached
pub struct Workflow {
    toolset: Arc<dyn Toolset>,
    engine: Arc<dyn InferenceEngine>,
    chain: Chain,
    tool_runner: ToolRunner,
    tracer: Option<TracingFacade>,
    event_sender: Sender<AgentToUiEvent>,
    conn: DbPool,
}

impl Workflow {
    /// Create a new workflow with permission checking enabled
    pub fn new(
        engine: Arc<dyn InferenceEngine>,
        permission_checker: Arc<crate::domain::permissions::PermissionChecker>,
        event_sender: Sender<AgentToUiEvent>,
        conn: DbPool,
    ) -> Result<Self, String> {
        Ok(Self {
            toolset: Arc::new(NoneToolset::new()),
            engine,
            chain: Chain::new(),
            tool_runner: ToolRunner::new(
                permission_checker,
                event_sender.clone(),
            ),
            tracer: None,
            event_sender,
            conn,
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
        mode: crate::domain::AgentModeType,
    ) -> Result<(), Error> {
        // Select toolset based on mode
        let toolset_type = match mode {
            crate::domain::AgentModeType::Build => ToolsetType::All,
            crate::domain::AgentModeType::Plan => ToolsetType::Discover,
        };

        // Get session_id for tools that need it
        let session_id = request.session_id().unwrap();

        self.toolset = toolset_type.build(
            request.user_settings().unwrap(),
            session_id,
            self.conn.clone(),
            self.event_sender.clone(),
        );
        self._init_tracer(request, "Code Generation Agentic Workflow");
        self._reset_chain();
        self.chain.add_history(request.get_history_steps());
        self.chain.set_todo_list(request.get_session_plan());

        // Add current user message as a step
        let user_step = ChainStep::user_message(
            request.current_request().to_string(),
            request.images().to_vec(),
        );
        self.chain.steps.push(user_step);

        let result = self._run(request, cancel, max_tool_calls, None);
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
        self.chain.add_history(request.get_history_steps());
        self.chain.set_todo_list(request.get_session_plan());

        // Add current user message as a step
        let user_step = ChainStep::user_message(
            request.current_request().to_string(),
            request.images().to_vec(),
        );
        self.chain.steps.push(user_step);

        let tree = GeneralTree::new();
        let mut current_step = Some(tree.head());

        while let Some(step) = current_step {
            let step_user_prompt =
                prompting::get_bt_tree_step_prompt(self.engine.get_type(), step, request);

            let session_id = request.session_id().unwrap();
            self.toolset = step.toolset().build(
                request.user_settings().unwrap(),
                session_id,
                self.conn.clone(),
                self.event_sender.clone(),
            );

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
                images: None,
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
        // Eternal agent loop
        for _iteration in 1..=max_tool_calls {
            // Check for cancellation at the start of each iteration
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                self.chain.add_interruption();
                return Ok(());
            }

            let system_prompt = prompting::get_system_prompt(self.engine.get_type(), request.mode());
            let base_user_prompt = prompting::get_user_prompt(self.engine.get_type(), request);
            let user_prompt = user_prompt_override
                .as_deref()
                .unwrap_or(&base_user_prompt)
                .to_string();

            self._emit_inference_progress();
            self._trace_llm_start(user_prompt.clone());

            // Ask LLM to choose next tool
            let llm_output = match self.engine.generate(
                &system_prompt,
                &user_prompt,
                1024,
                &self.toolset.tool_refs(),
                &self.chain,
                request.images(),
                self.tracer.as_mut(),
            ) {
                Ok(output) => output,
                Err(err) => {
                    return Err(Error::Inference(err.to_string()));
                }
            };

            self._trace_llm_end(llm_output.raw_output.clone());

            // Always capture the assistant's response in the chain
            if !llm_output.raw_output.is_empty() {
                self.chain.steps.push(ChainStep::assistant_response(
                    llm_output.summary.clone(),
                    llm_output.raw_output.clone(),
                ));
            }

            // Check for cancellation after inference
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                self.chain.add_interruption();
                return Ok(());
            }

            // Exit if there's no tool chosen - means we're done with the request
            if llm_output.tool_call.is_none() {
                let final_message = llm_output.summary;
                self.chain.set_final_message(final_message.clone());
                request.set_final_message(final_message.clone());
                self._end_tracing();

                return Ok(());
            }

            // Fallback for backward compatibility
            let tool_call = llm_output.tool_call.unwrap();
            let tool_result = self.tool_runner.run(
                request,
                tool_call,
                self.toolset.as_ref(),
                self.tracer.as_mut(),
            );
            self.chain.add_step(tool_result);
            continue;
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
        let mut tracer = TracingFacade::new(api_key, trace_name.to_string());
        tracer.start_span("Coding Agent", SpanKind::Agent);
        self.tracer = Some(tracer);
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

    fn _emit_inference_progress(&self) {
        let options = [
            "Thinking.. well kinda..",
            "Assembling answer..",
            "Consulting tokens..",
            "Plotting next step..",
            "Reasoning quietly..",
            "Spinning up ideas..",
            "Okay, now actually thinking..",
            "Reasoning..",
            "Looping ideas..",
            "Brainstorming..",
            "Predicting next symbols..",
            "Generating slop..",
        ];
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let index = (nanos % options.len() as u128) as usize;
        let _ = self
            .event_sender
            .send(AgentToUiEvent::ProgressEvent {
                step_name: "inference".to_string(),
                phase: StepPhase::Before,
                summary: options[index].to_string(),
            });
    }

    fn _trace_llm_end(&mut self, output: String) {
        if let Some(tracer) = &mut self.tracer {
            tracer.add_output("LLM generation", output);
            tracer.end_span("LLM generation");
        }
    }
}
