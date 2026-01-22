// module for behaviour trees definitions

mod general;

pub use general::GeneralTree;

use crate::domain::workflow::toolset::ToolsetType;

pub struct BTStepNode {
    prompt: String,
    toolset: ToolsetType,
    max_tools_calls: u32,
    next_step: Option<Box<dyn BTStepNodeInterface>>,
}

pub trait BTStepNodeInterface {
    fn prompt(&self) -> String;
    fn toolset(&self) -> ToolsetType;
    fn max_tools_calls(&self) -> u32;
    fn next_step(&self) -> Option<&dyn BTStepNodeInterface>;
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

    fn next_step(&self) -> Option<&dyn BTStepNodeInterface> {
        self.next_step.as_deref()
    }
}
