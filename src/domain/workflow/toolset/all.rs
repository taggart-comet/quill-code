use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use crate::domain::UserSettings;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct AllToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl AllToolset {
    pub fn new() -> Self {
        Self::new_with_settings(None)
    }

    pub fn new_with_settings(settings: Option<&UserSettings>) -> Self {
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

        if settings
            .map(|settings| settings.web_search_enabled())
            .unwrap_or(false)
        {
            let web_search = Box::new(WebSearch::new());
            tools.insert(web_search.name().to_string(), web_search);
        }

        Self { tools }
    }
}

impl Toolset for AllToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
