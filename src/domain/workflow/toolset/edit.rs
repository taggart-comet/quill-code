use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::AgentToUiEvent;
use crossbeam_channel::Sender;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct EditToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl EditToolset {
    pub fn new(session_id: i64, conn: DbPool, event_sender: Sender<AgentToUiEvent>) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let patch_files = Box::new(PatchFiles::new());
        tools.insert(patch_files.name().to_string(), patch_files);

        let update_todo = Box::new(UpdateTodoList::new(session_id, conn, event_sender));
        tools.insert(update_todo.name().to_string(), update_todo);

        Self { tools }
    }
}

impl Toolset for EditToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
