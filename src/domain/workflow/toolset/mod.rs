mod all;
mod edit;
mod read;
mod none;

pub use all::AllToolset;
pub use edit::EditToolset;
pub use read::ReadToolset;
pub use none::NoneToolset;
#[derive(Clone, Copy)]
pub enum ToolsetType {
    None,
    Read,
    Edit,
    All,
}

impl ToolsetType {
    pub fn build(self) -> Box<dyn Toolset> {
        match self {
            ToolsetType::Read => Box::new(ReadToolset::new()),
            ToolsetType::Edit => Box::new(EditToolset::new()),
            ToolsetType::All => Box::new(AllToolset::new()),
            ToolsetType::None => Box::new(NoneToolset::new()),
        }
    }
}

use crate::domain::tools::Tool;
use std::collections::HashMap;

/// Trait for toolset implementations that provide a set of tools
///
/// Implementations should create their tools in the `new()` constructor
/// and return a reference to them via the `tools()` method.
///
/// Common methods `get_tool`, `get_tools_description`, and `execute_tool`
/// have default implementations that work with any toolset.
pub trait Toolset {
    /// Returns a reference to the tools map
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>>;

    /// Get tool references for passing into inference engines
    fn tool_refs(&self) -> Vec<&dyn Tool> {
        self.tools().values().map(|tool| tool.as_ref()).collect()
    }
}
