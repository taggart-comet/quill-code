use crate::domain::bt::BTStepNodeInterface;
use crate::domain::session::Request;
use crate::domain::ModelType;
use crate::domain::prompting::get_user_prompt;

/// LLM prompt templates for the coding assistant
///
/// This module contains all prompt templates used for LLM interactions.

pub fn get_bt_tree_step_prompt(model_type: ModelType, step: &dyn BTStepNodeInterface, request: &dyn Request) -> String {
    if model_type == ModelType::OpenAI {
        format!(
            "Objective:\n{}\n\
            Current action:\n{}\n.",
            request.current_request(), step.prompt()
        )
    } else {
        format!(
            "{}\n{}", get_user_prompt(model_type, request), step.prompt()
        )
    }
}

pub fn get_system_prompt(model_type: ModelType) -> String {
    let (os_name, shell_name) = get_runtime_environment();
    if model_type == ModelType::OpenAI {
        format!(
            "You are Drastis, a coding agent. \n\
 Use the available tools to gather context and make changes. \
 When using tools, pass JSON arguments that match their parameters. \n\
 Web search policy: use at most 2 web_search calls per user request. \
 Ask for permission before accessing any new domain. \n\
 Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are Drastis, a coding agent. Use available tools to gather context and make changes. Be concise and accurate. Web search policy: use at most 2 web_search calls per user request and ask for permission before accessing any new domain. Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    }
}

fn get_runtime_environment() -> (String, String) {
    let os_name = std::env::consts::OS.to_string();
    let shell_name = std::env::var("SHELL")
        .ok()
        .and_then(|path| {
            std::path::Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    (os_name, shell_name)
}
