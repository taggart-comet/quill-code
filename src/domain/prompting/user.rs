use crate::domain::AgentModeType;
use crate::domain::ModelType;
use crate::domain::session::Request;
use crate::domain::todo::TodoListStatus;

/// Format request history and current request as a complete prompt
pub fn get_user_prompt(_model_type: ModelType, request: &dyn Request) -> String {
    if request.mode() == AgentModeType::BuildFromPlan {
        return _get_build_from_plan_prompt(request);
    }

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

fn _get_build_from_plan_prompt(request: &dyn Request) -> String {
    if let Some(todo_list) = request.get_session_plan() {
        for item in &todo_list.items {
            if item.status != TodoListStatus::Completed {
                return format!(
                    "Title: {}\n\nDescription:\n{}",
                    item.title, item.description
                );
            }
        }
    }
    "All TODO items are completed.".to_string()
}