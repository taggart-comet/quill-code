use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use crate::domain::UserSettings;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::AgentToUiEvent;
use crossbeam_channel::Sender;
use std::collections::HashMap;

pub struct AllNoTodoToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl AllNoTodoToolset {
    pub fn new(
        _session_id: i64,
        settings: &UserSettings,
        _conn: DbPool,
        _event_sender: Sender<AgentToUiEvent>
    ) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let discover_objects = Box::new(DiscoverObjects::new());
        tools.insert(discover_objects.name().to_string(), discover_objects);

        let read_objects = Box::new(ReadObjects::new());
        tools.insert(read_objects.name().to_string(), read_objects);

        let find_files = Box::new(FindFiles::new());
        tools.insert(find_files.name().to_string(), find_files);

        let structure = Box::new(Structure::new());
        tools.insert(structure.name().to_string(), structure);

        let patch_files = Box::new(PatchFiles::new());
        tools.insert(patch_files.name().to_string(), patch_files);

        let shell_exec = Box::new(ShellExec::new());
        tools.insert(shell_exec.name().to_string(), shell_exec);

        if settings.web_search_enabled() {
            let web_search = Box::new(WebSearch::new());
            tools.insert(web_search.name().to_string(), web_search);
        }

        Self { tools }
    }
}

impl Toolset for AllNoTodoToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}