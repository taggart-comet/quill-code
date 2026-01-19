use super::{chain::Chain, step::ChainStep, step::StepType, toolset::Toolset, CancellationToken, Error};
use crate::domain::bt::GeneralTree;
use crate::domain::prompting;
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use crate::domain::workflow::AllToolset;
use crate::infrastructure::inference::{local::LocalEngine, InferenceEngine};
use std::sync::Arc;

/// Main workflow orchestrator that runs LLM-driven coding tasks
/// Implements an eternal agent loop that:
/// 1. Asks LLM to choose the next tool
/// 2. Executes that tool
/// 3. Stores the result in the chain
/// 4. Repeats until 128 iterations reached
pub struct Workflow {
    toolset: Box<dyn Toolset>,
    engine: Arc<dyn InferenceEngine>,
    chain: Chain,
}

impl Workflow {
    /// Create a new workflow with default toolset (GeneralToolset)
    /// The engine is created internally by scanning and selecting a model
    pub fn new(engine: Arc<dyn InferenceEngine>) -> Result<Self, String> {
        Ok(Self {
            toolset: Box::new(AllToolset::new()),
            engine,
            chain: Chain::new(),
        })
    }

    pub fn reset_chain(&mut self) {
        self.chain = Chain::new();
    }

    pub fn chain(&self) -> &Chain {
        &self.chain
    }

    /// Run the workflow for a given request
    /// Implements the eternal agent loop:
    /// - Asks LLM to choose next tool
    /// - Executes the tool
    /// - Stores result in chain
    /// - Repeats until 128 iterations
    ///
    /// The workflow checks the cancellation token before each iteration
    /// and returns Error::Cancelled if cancellation was requested.
    pub fn run(
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

            let system_prompt = prompting::get_system_prompt(self.engine.get_type());
            let base_user_prompt = prompting::get_user_prompt(self.engine.get_type(), request);
            let user_prompt = user_prompt_override
                .as_deref()
                .unwrap_or(&base_user_prompt)
                .to_string();

            // Ask LLM to choose next tool
            let llm_output = self
                .engine
                .generate(
                    &system_prompt,
                    &user_prompt,
                    1024,
                    &self.toolset.tool_refs(),
                    &self.chain,
                )
                .map_err(|e| Error::Inference(e.to_string()))?;

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
                return Ok(());
            }

            let tool_result = llm_output.chosen_tool.unwrap().work(request);

            // Add step to chain
            self.chain.add_step(tool_result);
        }

        if max_tool_calls > 0 {
            log::warn!("Workflow reached maximum iterations ({})", max_tool_calls);
        }

        Ok(())
    }

    pub fn run_using_bt(
        &mut self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
    ) -> Result<Chain, Error> {
        self.reset_chain();
        let tree = GeneralTree::new();
        let mut current_step = Some(tree.head());

        while let Some(step) = current_step {
            let step_user_prompt =
                prompting::get_bt_tree_step_prompt(self.engine.get_type(), step, request);

            self.toolset = step.toolset().build();

            self.run(
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
            });
            if self.chain.is_failed {
                return Ok(self.chain.clone());
            }

            current_step = step.next_step();
        }

        Ok(self.chain.clone())
    }
}
