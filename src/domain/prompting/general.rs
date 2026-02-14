use crate::domain::bt::BTStepNodeInterface;
use crate::domain::prompting::user::get_user_prompt;
use crate::domain::session::Request;
use crate::domain::AgentModeType;
use crate::domain::ModelType;

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

pub fn get_system_prompt(
    model_type: ModelType,
    agent_mode: AgentModeType,
    remaining_calls: usize,
) -> String {
    let (os_name, shell_name) = get_runtime_environment();

    if agent_mode == AgentModeType::Plan {
        let mut system_prompt = _system_prompt_for_plan(model_type);
        if remaining_calls < 3 {
            system_prompt.push_str(&format!(
                "\n\nYou have {} tool calls left to process this request.",
                remaining_calls
            ));
        }
        return system_prompt;
    }
    if agent_mode == AgentModeType::BuildFromPlan {
        let mut system_prompt = _system_prompt_for_build_from_plan(model_type);
        if remaining_calls < 3 {
            system_prompt.push_str(&format!(
                "\n\nYou have {} tool calls left to process this request.",
                remaining_calls
            ));
        }
        return system_prompt;
    }
    let mut system_prompt = if model_type == ModelType::OpenAI {
        format!(
            "You're QuillCode, you're running as a coding agent in the CLI on a user's computer.\n\
You communicate concisely and pragmatically, keeping momentum toward the user's goal and avoiding unnecessary back-and-forth.\n\
In Default mode, strongly prefer executing the user's request rather than stopping to ask questions.\n\
Use the available tools to gather context and make changes.\n\
Important: a normal text response ends the request and performs no actions. If the task requires edits or commands, you must call tools in this turn.\n\
Do not describe actions; take them now with tools or ask a blocking question.\n\
When using tools, pass JSON arguments that match their parameters.\n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You're QuillCode, you're running as a coding agent in the CLI on a user's computer. Use available tools to gather context and make changes. Be concise and accurate. Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    };
    if remaining_calls < 3 {
        system_prompt.push_str(&format!(
            "\n\nYou have {} tool calls left to process this request.",
            remaining_calls
        ));
    }
    system_prompt
}

fn _system_prompt_for_plan(model_type: ModelType) -> String {
    let (os_name, shell_name) = get_runtime_environment();
    if model_type == ModelType::OpenAI {
        format!(
            "You're QuillCode, you're running as a coding agent in the CLI on a user's computer. \n\
You're in the Plan Mode! Use tools to gather all required information to make a detailed plan of what user wants to achieve. \n\
Ask the user, if there're any ambiguities, and clarify what needs to be done if the instructions are not clear. \n\
When you gathered the information, use the `update_todo_list` tool to create the TODO list and ask the user if he would like to make any changes to the TODO list. \n\
The user can see the TODO list in the interface once `update_todo_list` tool is used, so don't repeat it it response - respond with a general, summarized approach. \n\
Do not put into the TODO list the discovery and clarification steps, only the actions that need to be taken once the user confirms the plan. \n\
You're writing plan for yourself, so make it prompt-like, the end-result while you're in the Plan Mode is a comprehensive TODO list. \n\
IMPORTANT: Each TODO item will be executed independently by a sub-agent that has no memory of other items. \n\
The title should be a concise imperative action (e.g. \"Add retry logic to API client\"). \n\
The description MUST be a self-contained prompt that another agent can execute independently. Include: \n\
- Specific files to modify (full paths) \n\
- Exact changes needed (what to add, remove, or modify) \n\
- Code patterns and conventions to follow from the existing codebase \n\
- Acceptance criteria (what the result should look like) \n\
Write 1-3 detailed paragraphs with all necessary context so the sub-agent needs no additional information. \n\
Important: a normal text response ends the request and performs no actions. If the task requires tools, you must call tools in this turn.\n\
Do not describe actions; take them now with tools or ask a blocking question.\n\
When using tools, pass JSON arguments that match their parameters. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are QuillCode, a coding agent. \n\
You're in the Plan Mode! Use tools to gather all required information to make a detailed plan of what user wants to achieve. \n\
Ask the user, if there any ambiguities, and clarify what needs to be done if the instructions are not clear. \n\
When you gathered the information, use the `update_todo_list` tool to create the TODO list and ask the user if he would like to make any changes to the TODO list or is he ready to implement the plan. \n\
IMPORTANT: Each TODO item will be executed independently by a sub-agent that has no memory of other items. \n\
The title should be a concise imperative action. The description MUST be a self-contained prompt with: \n\
- Specific files to modify (full paths), exact changes needed, code patterns to follow, and acceptance criteria. \n\
Write 1-3 detailed paragraphs so the sub-agent needs no additional information. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    }
}

fn _system_prompt_for_build_from_plan(model_type: ModelType) -> String {
    let (os_name, shell_name) = get_runtime_environment();
    if model_type == ModelType::OpenAI {
        format!(
            "You are QuillCode, a coding agent executing a TODO list. \n\
You will receive one TODO item at a time via the user prompt. \n\
Before starting, mark the item as `in_progress` if it's still `pending`. Accomplish what is described in the current item using available tools, then mark it as `completed` via `update_todo_list` (preserve other items' statuses). \n\
After marking an item complete, continue working on the next pending item without stopping. \n\
When all items are done, stop without calling any more tools - just tell the user that you're done, and if you encountered any detours from the original plan. \n\
Important: a normal text response ends the request and performs no actions. If the task requires tools, you must call tools in this turn.\n\
Do not describe actions; take them now with tools or ask a blocking question.\n\
When using tools, pass JSON arguments that match their parameters. \n\
Runtime: os={}, shell={}.",
            os_name, shell_name
        )
    } else {
        format!(
            "You are QuillCode, a coding agent executing a TODO list. \n\
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
        "Below, in the json format is the current plan/TODO list for this session.\n\
This message gets auto-updated when `update_todo_list` tool is used - reflecting the change. When I say anything about the plan or todo-list, this list is meant.\n\
        \n\n```json\n{}\n```\n",
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
