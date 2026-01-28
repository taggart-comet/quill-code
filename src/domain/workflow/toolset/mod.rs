mod all;
mod edit;
mod none;
mod discover;

use std::collections::HashMap;
use std::sync::Arc;
use crossbeam_channel::Sender;
pub use all::AllToolset;
pub use edit::EditToolset;
pub use none::NoneToolset;
pub use discover::DiscoverToolset;
use crate::infrastructure::db::DbPool;
use crate::domain::tools::Tool;
use crate::domain::UserSettings;
use crate::infrastructure::event_bus::AgentToUiEvent;
use crate::infrastructure::inference::ToolCall;
use crate::domain::tools::Error;

#[derive(Clone, Copy)]
pub enum ToolsetType {
    None,
    Discover,
    Edit,
    All,
}

impl ToolsetType {
    pub fn build(
        self,
        settings: &UserSettings,
        session_id: i64,
        conn: DbPool,
        event_sender: Sender<AgentToUiEvent>,
    ) -> Arc<dyn Toolset> {
        match self {
            ToolsetType::Discover => Arc::new(DiscoverToolset::new(session_id, conn, event_sender)),
            ToolsetType::Edit => Arc::new(EditToolset::new(session_id, conn, event_sender)),
            ToolsetType::All => Arc::new(AllToolset::new(session_id, settings, conn, event_sender)),
            ToolsetType::None => Arc::new(NoneToolset::new()),
        }
    }
}

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

    /// Get a specific tool by name
    fn prepare_tool(&self, tool_call: &ToolCall) -> Result<&dyn Tool, Error> {
        let tool = self
            .tools()
            .get(&tool_call.name)
            .map(|t| t.as_ref())
            .ok_or_else(|| Error::Parse(format!("Tool not found: {}", tool_call.name)))?;

        if let Some(err) = tool.parse_input(tool_call.arguments.clone()) {
            return Err(err);
        }

        Ok(tool)
    }
}
