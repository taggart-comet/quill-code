use super::{BTStepNode, BTStepNodeInterface};
use crate::domain::workflow::toolset::ToolsetType;

pub struct GeneralTree {
    head: Box<dyn BTStepNodeInterface>,
}

impl GeneralTree {
    pub fn new() -> Self {
        GeneralTree{
            head: Box::new(BTStepNode {
                prompt: "Locate impact area: list likely files/modules, then read only the minimal code needed to confirm APIs, types, and behavior.".to_string(),
                toolset: ToolsetType::Read,
                max_tools_calls: 4,
                next_step: Some(Box::new(BTStepNode{
                    prompt: "Design the change: choose between extending existing modules vs new code, \
                        prefer minimal changes, choose to write the functionality or there's a good opensource library for that. Don't forget to plan adding or changing tests.\
                         Outline the `change plan` - list of files that should be changed and how they should be changed.".to_string(),
                    toolset: ToolsetType::Read,
                    max_tools_calls: 2,
                    next_step: Some(Box::new(BTStepNode{
                        prompt: "Collect the necessary information to build the patch request for the files from the `change plan`. You have maximum of 8 tool/function calls available.".to_string(),
                        toolset: ToolsetType::Read,
                        max_tools_calls: 8,
                        next_step: Some(Box::new(BTStepNode {
                            prompt: "Implement the patch according to the `change plan`: follow project style, and update related code paths as needed.".to_string(),
                            toolset: ToolsetType::Edit,
                            max_tools_calls: 3,
                            next_step: Some(Box::new(BTStepNode {
                                prompt: "Run relevant checks: build/compile or linters/tests if applicable; capture errors and be ready to fix them.".to_string(),
                                toolset: ToolsetType::All,
                                max_tools_calls: 8,
                                next_step: Some(Box::new(BTStepNode {
                                    prompt: "Summarize changes for the user: list key updates, touched files, and tests run. Keep it short.".to_string(),
                                    toolset: ToolsetType::None,
                                    max_tools_calls: 1,
                                    next_step: None,
                                })),
                            })),
                        })),
                    })),
                })),
            })
        }
    }

    pub fn head(&self) -> &dyn BTStepNodeInterface {
        self.head.as_ref()
    }
}

impl Default for GeneralTree {
    fn default() -> Self {
        Self::new()
    }
}
