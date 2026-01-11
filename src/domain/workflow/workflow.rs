use super::{chain::Chain, toolset::Toolset, CancellationToken, Error};
use crate::domain::prompting;
use crate::domain::session::Session;
use crate::domain::tools::ToolResult;
use crate::infrastructure::inference::InferenceEngine;

/// Main workflow orchestrator that runs LLM-driven coding tasks
/// Implements an eternal agent loop that:
/// 1. Asks LLM to choose the next tool
/// 2. Executes that tool
/// 3. Stores the result in the chain
/// 4. Repeats until "finish" tool is chosen or 128 iterations reached
pub struct Workflow {
    toolset: Toolset,
}

impl Workflow {
    /// Create a new workflow with default toolset
    pub fn new() -> Self {
        Self {
            toolset: Toolset::new(),
        }
    }

    /// Create a workflow with a custom toolset
    pub fn with_toolset(toolset: Toolset) -> Self {
        Self { toolset }
    }

    /// Run the workflow for a given session
    /// Implements the eternal agent loop:
    /// - Asks LLM to choose next tool
    /// - Executes the tool
    /// - Stores result in chain
    /// - Repeats until "finish" or 128 iterations
    ///
    /// The workflow checks the cancellation token before each iteration
    /// and returns Error::Cancelled if cancellation was requested.
    pub fn run(
        &self,
        session: &Session,
        engine: &InferenceEngine,
        cancel: &CancellationToken,
    ) -> Result<Chain, Error> {
        const MAX_ITERATIONS: usize = 128;

        // Get the user prompt from the session
        let user_prompt = prompting::format_session_prompt(&session);

        // Initialize the chain to track executed steps
        let mut chain = Chain::new();

        // Eternal agent loop
        for iteration in 1..=MAX_ITERATIONS {
            // Check for cancellation at the start of each iteration
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                chain.add_interruption();
                return Ok(chain);
            }

            // Create prompt with chain context
            let prompt = prompting::tool_selection_prompt(&user_prompt, &self.toolset, &chain);

            // Ask LLM to choose next tool
            let llm_output = engine.generate_silent(&prompt, 1024)
                .map_err(Error::Inference)?;

            // Check for cancellation after inference
            if cancel.is_cancelled() {
                log::info!("Workflow cancelled by user");
                chain.add_interruption();
                return Ok(chain);
            }

            // Parse tool choice from LLM output
            let (tool_name, input_yaml) = super::chain::parse_tool_choice(&llm_output)?;

            // Check if we should finish
            if tool_name == "finish" {
                log::info!("Finish tool chosen, ending workflow");
                break;
            }

            // Execute the tool
            let tool_result = match self.toolset.execute_tool(&tool_name, input_yaml.clone()) {
                Ok(result) => {
                    log::info!("Tool {} executed successfully", tool_name);
                    result
                }
                Err(error) => {
                    log::warn!("Tool {} failed: {}", tool_name, error);
                    let error_msg = format!("Tool execution failed: {}", error);
                    // Create an error ToolResult for consistency
                    let result = ToolResult::error(
                        tool_name.clone(),
                        input_yaml.clone(),
                        error_msg.clone(),
                    );
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
    pub fn toolset(&self) -> &Toolset {
        &self.toolset
    }
}

impl Default for Workflow {
    fn default() -> Self {
        Self::new()
    }
}
