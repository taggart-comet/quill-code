// module for behaviour trees definitions

mod general;

pub use general::GeneralTree;

use crate::domain::workflow::toolset::ToolsetType;

pub enum BTNodeStatus {
    Pending,
    Running,
    Success,
    Failure,
}

pub trait BehaviorTree {
    fn get_next_step(&self) -> Option<&BTStepNode>;
}

pub struct BTStepNode {
    prompt: String,
    status: BTNodeStatus,
    toolset: ToolsetType,
    max_tools_calls: u32,
    max_retries: u32,
    next_step: Option<Box<dyn BTStepNodeInterface>>,
    decision: Option<Box<dyn BTDecisionNodeInterface>>,
}

pub trait BTStepNodeInterface {
    fn prompt(&self) -> String;
    fn toolset(&self) -> ToolsetType;
    fn max_tools_calls(&self) -> u32;
    fn max_retries(&self) -> u32;
    fn next_step(&self) -> Option<&dyn BTStepNodeInterface>;
    fn has_decision(&self) -> bool;
    fn get_decision(&self) -> Option<&dyn BTDecisionNodeInterface>;
}

pub struct BTDecisionNode {
    prompt: String,
    status: BTNodeStatus,
    option_a: BTStepNode,
    option_b: BTStepNode,
}

pub trait BTDecisionNodeInterface {
    fn prompt(&self) -> String;
    fn get_next(&self, llm_output: &str) -> Option<&BTStepNode>;
}

impl BTStepNodeInterface for BTStepNode {
    fn prompt(&self) -> String {
        self.prompt.clone()
    }

    fn toolset(&self) -> ToolsetType {
        self.toolset
    }

    fn max_tools_calls(&self) -> u32 {
        self.max_tools_calls
    }

    fn max_retries(&self) -> u32 {
        self.max_retries
    }

    fn next_step(&self) -> Option<&dyn BTStepNodeInterface> {
        self.next_step.as_deref()
    }

    fn has_decision(&self) -> bool {
        self.decision.is_some()
    }

    fn get_decision(&self) -> Option<&dyn BTDecisionNodeInterface> {
        self.decision.as_deref()
    }
}
