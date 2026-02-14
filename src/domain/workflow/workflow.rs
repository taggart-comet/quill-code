use super::{
    chain::Chain, step::ChainStep, step::StepType, tool_runner::ToolRunner, toolset::Toolset,
    CancellationToken, Error,
};
use crate::domain::bt::GeneralTree;
use crate::domain::prompting;
use crate::domain::session::Request;
use crate::domain::todo::TodoListStatus;
use crate::domain::workflow::toolset::NoneToolset;
use crate::domain::workflow::toolset::ToolsetType;
use crate::domain::AgentModeType;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{AgentToUiEvent, StepPhase};
use crate::infrastructure::inference::InferenceEngine;
use crossbeam_channel::Sender;
use openai_agents_tracing::{SpanKind, TracingFacade};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::{Builder, Handle};

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
    initial_toolset_type: Option<ToolsetType>,
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
            tool_runner: ToolRunner::new(permission_checker, event_sender.clone()),
            tracer: None,
            event_sender,
            conn,
            initial_toolset_type: None,
        })
    }

    pub fn get_chain(&self) -> &Chain {
        &self.chain
    }

    pub fn run(
        &mut self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
        mode: AgentModeType,
    ) -> Result<(), Error> {
        // Select toolset based on mode
        let toolset_type = match mode {
            AgentModeType::Build => ToolsetType::AllNoTodo,
            AgentModeType::Plan => ToolsetType::Discover,
            AgentModeType::BuildFromPlan => ToolsetType::All,
        };

        // Store initial toolset type for finishing mode logic
        self.initial_toolset_type = Some(toolset_type);

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

        let result = self._run(request, cancel, None, None, mode);
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
            let mode = request.mode();
            self.toolset = step.toolset().build(
                request.user_settings().unwrap(),
                session_id,
                self.conn.clone(),
                self.event_sender.clone(),
            );

            self._run(
                request,
                cancel,
                Some(step.max_tools_calls() as usize),
                Some(step_user_prompt.clone()),
                mode,
            )?;
            self.chain.steps.push(ChainStep {
                step_type: StepType::BehaviorTreeStepPassed.as_str().to_string(),
                summary: self.chain.final_message().unwrap_or("").to_string(),
                context_payload: String::new(),
                input_payload: step_user_prompt.to_string(),
                tool_name: None,
                call_id: None,
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
        max_tool_calls_override: Option<usize>,
        user_prompt_override: Option<String>,
        mode: AgentModeType,
    ) -> Result<(), Error> {
        // Inject AGENTS.md as first user message if chain is empty and file exists
        if self.chain.steps.is_empty() {
            if let Some(agents_content) = load_agents_prompt(request.project_root()) {
                self.chain
                    .steps
                    .push(ChainStep::user_message(agents_content, vec![]));
            }
        }

        let base_user_prompt = prompting::get_user_prompt(self.engine.get_type(), request);
        let user_prompt = user_prompt_override
            .as_deref()
            .unwrap_or(&base_user_prompt)
            .to_string();
        self.chain.steps.push(ChainStep::user_message(
            user_prompt.clone(),
            request.images().to_vec(),
        ));

        // Get max_tool_calls from override (for BT mode) or user settings
        let max_tool_calls = max_tool_calls_override.unwrap_or_else(|| {
            request
                .user_settings()
                .map(|s| s.max_tool_calls_per_request() as usize)
                .unwrap_or(50)
        });

        // Initialize counter and tracking variables
        let mut tool_call_count = 0;
        let mut current_active_todo = self.chain.todo_list.clone();
        let mut in_finishing_mode = false;
        let finishing_threshold = if max_tool_calls > 5 {
            max_tool_calls - 5
        } else {
            max_tool_calls
        };

        // Eternal agent loop
        for _iteration in 1..=max_tool_calls {
            // Check for cancellation at the start of each iteration
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                self.chain.add_interruption();
                return Ok(());
            }

            // Calculate remaining calls
            let remaining_calls = max_tool_calls.saturating_sub(tool_call_count);

            // Get base system prompt and inject remaining count
            let system_prompt = prompting::get_system_prompt(
                self.engine.get_type(),
                request.mode(),
                remaining_calls,
            );
            self.chain.set_system_prompt(system_prompt.clone());

            // Switch to finishing toolset if approaching limit
            if !in_finishing_mode && tool_call_count >= finishing_threshold {
                if let Some(initial_type) = self.initial_toolset_type {
                    let finishing_type = initial_type.finishing_variant();
                    let session_id = request.session_id().unwrap();
                    self.toolset = finishing_type.build(
                        request.user_settings().unwrap(),
                        session_id,
                        self.conn.clone(),
                        self.event_sender.clone(),
                    );
                    in_finishing_mode = true;
                }
            }

            self._emit_inference_progress();

            // Ask LLM to choose next tool
            let llm_output = match self.engine.generate(
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
            let is_update_todo = tool_call.name == "update_todo_list";

            let tool_result = self.tool_runner.run(
                request,
                tool_call,
                self.toolset.as_ref(),
                self.tracer.as_mut(),
            );
            self.chain.add_step(tool_result);

            // Increment counter after tool execution
            tool_call_count += 1;

            // Check if TODO item changed and reset counter if so (BuildFromPlan mode only)
            if mode == AgentModeType::BuildFromPlan && is_update_todo {
                if self._did_todo_item_change(&current_active_todo) {
                    log::info!(
                        "Active TODO item changed, resetting counter from {} to 0",
                        tool_call_count
                    );
                    tool_call_count = 0;
                    current_active_todo = self.chain.todo_list.clone();

                    // Exit finishing mode and restore full toolset
                    if in_finishing_mode {
                        log::info!("Exiting finishing mode, restoring full toolset");
                        if let Some(initial_type) = self.initial_toolset_type {
                            let session_id = request.session_id().unwrap();
                            self.toolset = initial_type.build(
                                request.user_settings().unwrap(),
                                session_id,
                                self.conn.clone(),
                                self.event_sender.clone(),
                            );
                            in_finishing_mode = false;
                        }
                    }
                }
            }

            continue;
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
        let _ = self.event_sender.send(AgentToUiEvent::ProgressEvent {
            step_name: "inference".to_string(),
            phase: StepPhase::Before,
            summary: options[index].to_string(),
        });
    }

    /// Detects if the active TODO item has changed
    /// Returns true if the first non-completed item's title is different
    fn _did_todo_item_change(&self, previous_todo: &Option<crate::domain::todo::TodoList>) -> bool {
        // Get first non-completed item from previous TODO list
        let prev_active = previous_todo.as_ref().and_then(|list| {
            list.items
                .iter()
                .find(|item| item.status != TodoListStatus::Completed)
                .map(|item| &item.title)
        });

        // Get first non-completed item from current TODO list
        let curr_active = self.chain.todo_list.as_ref().and_then(|list| {
            list.items
                .iter()
                .find(|item| item.status != TodoListStatus::Completed)
                .map(|item| &item.title)
        });

        // If both are None, no change
        // If one is None and the other isn't, change occurred
        // If both exist but titles differ, change occurred
        prev_active != curr_active
    }
}

fn load_agents_prompt(project_root: &Path) -> Option<String> {
    let agents_path = project_root.join("AGENTS.md");
    if agents_path.is_file() {
        std::fs::read_to_string(&agents_path)
            .ok()
            .filter(|content| !content.is_empty())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_agents_prompt_file_exists() {
        let dir = tempdir().unwrap();
        let agents_path = dir.path().join("AGENTS.md");
        std::fs::write(&agents_path, "You are a helpful agent.").unwrap();

        let result = load_agents_prompt(dir.path());
        assert_eq!(result, Some("You are a helpful agent.".to_string()));
    }

    #[test]
    fn test_load_agents_prompt_no_file() {
        let dir = tempdir().unwrap();
        let result = load_agents_prompt(dir.path());
        assert_eq!(result, None);
    }

    #[test]
    fn test_load_agents_prompt_empty_file() {
        let dir = tempdir().unwrap();
        let agents_path = dir.path().join("AGENTS.md");
        std::fs::write(&agents_path, "").unwrap();

        let result = load_agents_prompt(dir.path());
        assert_eq!(result, None);
    }
}
