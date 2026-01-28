use crate::domain::AgentModeType;
use crate::domain::bt::BTStepNodeInterface;
use crate::domain::session::Request;
use crate::domain::ModelType;
use crate::domain::prompting::user::get_user_prompt;

/// LLM prompt templates for the coding assistant
///
/// This module contains all prompt templates used for LLM interactions.

pub fn get_bt_tree_step_prompt(
    model_type: ModelType,
    step: &dyn BTStepNodeInterface,
    request: &dyn Request,
) -> String {
    if model_type == ModelType::OpenAI {
        format!(
            "Objective:\n{}\n\
            Current action:\n{}\n.",
            request.current_request(),
            step.prompt()
        )
    } else {
        format!(
            "{}\n{}",
            get_user_prompt(model_type, request),
            step.prompt()
        )
    }
}

pub fn get_system_prompt(model_type: ModelType, agent_mode: AgentModeType) -> String {
    let (os_name, shell_name) = get_runtime_environment();

    if agent_mode == AgentModeType::Plan {
        return _system_prompt_for_plan(model_type);
    }
    if agent_mode == AgentModeType::BuildFromPlan {
        return _system_prompt_for_build_from_plan(model_type);
    }
    if model_type == ModelType::OpenAI {
        format!(
            "You are Drastis, a coding agent. \n\
 Use the available tools to gather context and make changes. \
 When using tools, pass JSON arguments that match their parameters. \n\
 Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are Drastis, a coding agent. Use available tools to gather context and make changes. Be concise and accurate. Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    }
}

fn _system_prompt_for_plan(model_type: ModelType) -> String {
    let (os_name, shell_name) = get_runtime_environment();
    if model_type == ModelType::OpenAI {
        format!(
            "You are Drastis, a coding agent. \n\
You're in the Plan Mode! Use tools to gather all required information to make a detailed plan of what user wants to achieve. \n\
Ask the user, if there're any ambiguities, and clarify what needs to be done if the instructions are not clear. \n\
When you gathered the information, use the `update_todo_list` tool to create the TODO list and ask the user if he would like to make any changes to the TODO list. \n\
The user can see the TODO list in the interface once `update_todo_list` tool is used, so don't repeat it it response - respond with a general, summarized approach. \n\
Do not put into the TODO list the discovery and clarification steps, only the actions that need to be taken once the user confirms the plan. \n\
You're writing plan for yourself, so make it prompt-like, the end-result while you're in the Plan Mode is a comprehensive TODO list. \n\
When using tools, pass JSON arguments that match their parameters. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are Drastis, a coding agent. \n\
You're in the Plan Mode! Use tools to gather all required information to make a detailed plan of what user wants to achieve. \n\
Ask the user, if there any ambiguities, and clarify what needs to be done if the instructions are not clear. \n\
When you gathered the information, use the `update_todo_list` tool to create the TODO list and ask the user if he would like to make any changes to the TODO list or is he ready to implement the plan. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    }
}

fn _system_prompt_for_build_from_plan(model_type: ModelType) -> String {
    let (os_name, shell_name) = get_runtime_environment();
    if model_type == ModelType::OpenAI {
        format!(
            "You are Drastis, a coding agent executing a TODO list. \n\
You will receive one TODO item at a time via the user prompt. \n\
Before starting, mark the item as `in_progress` if it's still `pending`. Accomplish what is described in the current item using available tools, then mark it as `completed` via `update_todo_list` (preserve other items' statuses). \n\
After marking an item complete, continue working on the next pending item without stopping. \n\
When all items are done, stop without calling any more tools - just tell the user that you're done, and if you encountered any detours from the original plan. \n\
When using tools, pass JSON arguments that match their parameters. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are Drastis, a coding agent executing a TODO list. \n\
You will receive one TODO item at a time via the user prompt. \n\
Complete the current item using available tools, then mark it as `completed` via `update_todo_list` (preserve other items' statuses). \n\
After marking an item complete, continue working on the next pending item without stopping. \n\
When all items are done, stop without calling any more tools. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    }
}

pub fn format_todo_list_message(todo_content: &str) -> String {
    format!(
        "## Current Session TODO List\n\nBelow is the current plan/TODO list for this session:\n\n```json\n{}\n```\n",
        todo_content
    )
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
