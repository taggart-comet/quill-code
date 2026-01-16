use super::{chain::Chain, toolset::Toolset, CancellationToken, Error};
use crate::domain::prompting;
use crate::domain::session::Request;
use crate::domain::tools::ToolResult;
use crate::domain::workflow::GeneralToolset;
use crate::infrastructure::inference::{local::LocalEngine, InferenceEngine};
use std::sync::Arc;

/// Main workflow orchestrator that runs LLM-driven coding tasks
/// Implements an eternal agent loop that:
/// 1. Asks LLM to choose the next tool
/// 2. Executes that tool
/// 3. Stores the result in the chain
/// 4. Repeats until "finish" tool is chosen or 128 iterations reached
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
    /// - Repeats until "finish" or 128 iterations
    ///
    /// The workflow checks the cancellation token before each iteration
    /// and returns Error::Cancelled if cancellation was requested.
    pub fn run(&self, request: &dyn Request, cancel: &CancellationToken) -> Result<Chain, Error> {
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

            // Create prompt with chain context
            let prompt = prompting::main_request_prompt(
                self.engine.get_type(),
                request,
                self.toolset.as_ref(),
                &chain,
            );

            // Ask LLM to choose next tool
            let llm_output = self
                .engine
                .generate(&prompt, 1024)
                .map_err(Error::Inference)?;

            // Check for cancellation after inference
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                chain.add_interruption();
                return Ok(chain);
            }

            // Parse tool choice from LLM output
            let (tool_name, input_xml) = super::chain::parse_tool_choice(&llm_output)?;

            // Check if we should finish
            if tool_name == "finish" {
                log::info!("Finish tool chosen, ending workflow");
                break;
            }

            // Execute the tool
            let tool_result = match self.toolset.execute_tool(&tool_name, &input_xml, request) {
                Ok(result) => {
                    log::info!("Tool {} executed successfully", tool_name);
                    result
                }
                Err(error) => {
                    log::warn!("Tool {} failed: {}", tool_name, error);
                    let error_msg = format!("Tool execution failed: {}", error);
                    // Create an error ToolResult for consistency
                    let tool_input = crate::domain::tools::ToolInput::new(&input_xml)
                        .unwrap_or_else(|_| {
                            crate::domain::tools::ToolInput::new("<input></input>").unwrap()
                        });
                    let result =
                        ToolResult::error(tool_name.clone(), &tool_input, error_msg.clone());
                    chain.add_step(result);
                    chain.mark_failed(error_msg);
                    return Ok(chain);
                }
            };

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
