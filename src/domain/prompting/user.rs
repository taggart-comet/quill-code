use crate::domain::AgentModeType;
use crate::domain::ModelType;
use crate::domain::session::Request;
use crate::domain::todo::TodoListStatus;

/// Format request history and current request as a complete prompt
pub fn get_user_prompt(_model_type: ModelType, request: &dyn Request) -> String {
    if request.mode() == AgentModeType::BuildFromPlan {
        return _get_build_from_plan_prompt(request);
    }
    request.current_request().to_string()
}

fn _get_build_from_plan_prompt(request: &dyn Request) -> String {
    if let Some(todo_list) = request.get_session_plan() {
        for item in &todo_list.items {
            if item.status != TodoListStatus::Completed {
                return format!(
                    "TODO Item Title: {}\n\nDescription:\n{}",
                    item.title, item.description
                );
            }
        }
    }
    "Report very briefly what was done without using any tools, since all the TODO items are completed.".to_string()
}