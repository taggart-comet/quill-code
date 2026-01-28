use crate::domain::tools::Tool;
use crate::domain::workflow::Chain;
use crate::domain::ModelType;
use crate::infrastructure::InfaError;

pub mod local;
pub mod openai;

pub struct ToolCall {
    pub name: String,
    pub arguments: String,
}

pub struct LLMInferenceResult {
    pub summary: String,
    pub raw_output: String,
    pub tool_call: Option<ToolCall>,
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
        images: &[String],
        tracer: Option<&mut openai_agents_tracing::TracingFacade>,
    ) -> Result<LLMInferenceResult, InfaError>;
    fn get_type(&self) -> ModelType;
}
