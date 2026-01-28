use crate::domain::ModelType;

/// Create a prompt for generating a short session name
pub fn session_naming_prompt(model_type: ModelType, prompt_preview: &str) -> String {
    if model_type == ModelType::OpenAI {
        format!(
            "Generate a short title (3-5 words) for the following request: {}\n\
            Respond with ONLY the title itself. Do not add explanations, punctuation, or extra text.",
            prompt_preview
        )
    } else {
        format!(
            "<|im_start|>system\nYou generate very short titles (3-5 words). Respond with ONLY the title itself, without any extra text.<|im_end|>\n\
            <|im_start|>user\nTitle for: {}<|im_end|>\n\
            <|im_start|>assistant\n",
            prompt_preview
        )
    }
}
