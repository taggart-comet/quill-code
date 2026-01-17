use crate::domain::tools::Tool;
use crate::domain::workflow::Chain;
use crate::domain::ModelType;
use crate::infrastructure::InfaError;

pub mod local;
pub mod openai;

pub struct LLMInferenceResult {
    pub summary: String,
    pub chosen_tool: Option<Box<dyn Tool>>,
}

/// Common interface for inference engines
pub trait InferenceEngine: Send + Sync {
    /// Generate text without streaming output
    fn generate(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
        tools: &[&dyn Tool],
        chain: &Chain,
    ) -> Result<LLMInferenceResult, InfaError>;
    fn get_type(&self) -> ModelType;
}
