use crate::domain::session::Request;
use crate::domain::ModelType;

/// Format request history and current request as a complete prompt
pub fn get_user_prompt(_model_type: ModelType, request: &dyn Request) -> String {
    let mut formatted = String::new();

    // Add history section
    formatted.push_str("previous requests history:\n");

    if request.history().is_empty() {
        formatted.push_str("  (no previous requests)\n");
    } else {
        for req in request.history() {
            formatted.push_str(&format!("  - request: {}\n", req.prompt()));
            if let Some(result) = req.result_summary() {
                formatted.push_str(&format!("    result: {}\n", result));
            } else {
                formatted.push_str("    result: (no result yet)\n");
            }
        }
    }

    // Add current request
    formatted.push_str(&format!("CURRENT REQUEST: {}", request.current_request()));

    formatted
}

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