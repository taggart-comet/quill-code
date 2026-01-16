use crate::domain::tools::Tool;
use crate::domain::ModelType;
use crate::infrastructure::InfaError;

pub mod local;
pub mod openai;

pub struct LLMInferenceResult {
    pub reasoning_text: String,
    pub summary: String,
    pub chosen_tool: Option<dyn Tool>,
}

/// Common interface for inference engines
pub trait InferenceEngine: Send + Sync {
    /// Generate text without streaming output
    fn generate(&self, prompt: &str, max_tokens: u32) -> Result<LLMInferenceResult, InfaError>;
    fn get_type(&self) -> ModelType;
}
