mod all;
mod edit;
mod none;
mod read;

pub use all::AllToolset;
pub use edit::EditToolset;
pub use none::NoneToolset;
pub use read::ReadToolset;
#[derive(Clone, Copy)]
pub enum ToolsetType {
    None,
    Read,
    Edit,
    All,
}

impl ToolsetType {
    pub fn build(self) -> Box<dyn Toolset> {
        self.build_with_settings(None)
    }

    pub fn build_with_settings(
        self,
        settings: Option<&crate::domain::UserSettings>,
    ) -> Box<dyn Toolset> {
        match self {
            ToolsetType::Read => Box::new(ReadToolset::new()),
            ToolsetType::Edit => Box::new(EditToolset::new()),
            ToolsetType::All => Box::new(AllToolset::new_with_settings(settings)),
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
