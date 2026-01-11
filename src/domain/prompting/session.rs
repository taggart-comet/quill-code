use crate::domain::Session;

/// Format session history and current request as a complete prompt
pub fn format_session_prompt(session: &Session) -> String {
    let mut formatted = String::new();

    // Add history section
    formatted.push_str("previous requests history:\n");

    if session.requests().is_empty() {
        formatted.push_str("  (no previous requests)\n");
    } else {
        for request in session.requests() {
            formatted.push_str(&format!("  - request: {}\n", request.prompt()));
            if let Some(result) = request.result_summary() {
                formatted.push_str(&format!("    result: {}\n", result));
            } else {
                formatted.push_str("    result: (no result yet)\n");
            }
        }
    }

    // Add current request
    formatted.push_str(&format!("CURRENT REQUEST: {}", session.current_request()));

    formatted
}

/// Create a prompt for generating a short session name
pub fn session_naming_prompt(prompt_preview: &str) -> String {
    format!(
        "<|im_start|>system\nYou generate very short titles (3-5 words). Respond with only the title.<|im_end|>\n\
        <|im_start|>user\nTitle for: {}<|im_end|>\n\
        <|im_start|>assistant\n",
        prompt_preview
    )
}