use super::{chain::Chain, toolset::Toolset, CancellationToken, Error};
use crate::domain::prompting;
use crate::domain::session::Request;
use crate::domain::tools::Tool;
use crate::domain::workflow::GeneralToolset;
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
}

impl Workflow {
    /// Create a new workflow with default toolset (GeneralToolset)
    /// The engine is created internally by scanning and selecting a model
    pub fn new(engine: Arc<dyn InferenceEngine>) -> Result<Self, String> {
        Ok(Self {
            toolset: Box::new(GeneralToolset::new()),
            engine,
        })
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
        &self,
        request: &mut dyn Request,
        cancel: &CancellationToken,
    ) -> Result<Chain, Error> {
        const MAX_ITERATIONS: usize = 128;

        // Initialize the chain to track executed steps
        let mut chain = Chain::new();

        // Eternal agent loop
        for _iteration in 1..=MAX_ITERATIONS {
            // Check for cancellation at the start of each iteration
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                chain.add_interruption();
                return Ok(chain);
            }

            let system_prompt = prompting::get_system_prompt(self.engine.get_type());
            let user_prompt = prompting::get_user_prompt(self.engine.get_type(), request);

            // Ask LLM to choose next tool
            let llm_output = self
                .engine
                .generate(
                    &system_prompt,
                    &user_prompt,
                    1024,
                    &self.toolset.tool_refs(),
                    &chain,
                )
                .map_err(|e| Error::Inference(e.to_string()))?;

            // Check for cancellation after inference
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                chain.add_interruption();
                return Ok(chain);
            }

            if llm_output.chosen_tool.is_none() {
                let final_message = llm_output.summary;
                chain.set_final_message(final_message.clone());
                request.set_final_message(final_message);
                return Ok(chain);
            }

            let tool_result = llm_output.chosen_tool.unwrap().work(request);

            // Add step to chain
            chain.add_step(tool_result);
        }

        if chain.len() >= MAX_ITERATIONS {
            log::warn!("Workflow reached maximum iterations ({})", MAX_ITERATIONS);
            chain.mark_failed("Maximum iterations reached".to_string());
        }

        Ok(chain)
    }

    /// Get the toolset (for testing or inspection)
    pub fn toolset(&self) -> &dyn Toolset {
        self.toolset.as_ref()
    }
}
