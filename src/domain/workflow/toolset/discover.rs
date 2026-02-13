use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::AgentToUiEvent;
use crossbeam_channel::Sender;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct DiscoverToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl DiscoverToolset {
    pub fn new(session_id: i64, conn: DbPool, event_sender: Sender<AgentToUiEvent>) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let discover_objects = Box::new(DiscoverObjects::new());
        tools.insert(discover_objects.name().to_string(), discover_objects);

        let read_objects = Box::new(ReadObjects::new());
        tools.insert(read_objects.name().to_string(), read_objects);

        let find_files = Box::new(FindFiles::new());
        tools.insert(find_files.name().to_string(), find_files);

        let structure = Box::new(Structure::new());
        tools.insert(structure.name().to_string(), structure);

        let update_todo = Box::new(UpdateTodoList::new(session_id, conn, event_sender));
        tools.insert(update_todo.name().to_string(), update_todo);

        Self { tools }
    }
}

impl Toolset for DiscoverToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
