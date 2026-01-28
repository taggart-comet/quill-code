use crate::domain::ModelType;
use crate::domain::session::Request;

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