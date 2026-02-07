use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::AgentToUiEvent;
use crossbeam_channel::Sender;
use std::collections::HashMap;

pub struct FinishingAllNoTodoToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl FinishingAllNoTodoToolset {
    pub fn new(
        _session_id: i64,
        _conn: DbPool,
        _event_sender: Sender<AgentToUiEvent>
    ) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let patch_files = Box::new(PatchFiles::new());
        tools.insert(patch_files.name().to_string(), patch_files);

        Self { tools }
    }
}

impl Toolset for FinishingAllNoTodoToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}