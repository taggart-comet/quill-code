use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct GeneralToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl GeneralToolset {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let discover_objects = Box::new(DiscoverObjects);
        tools.insert(discover_objects.name().to_string(), discover_objects);

        let read_objects = Box::new(ReadObjects);
        tools.insert(read_objects.name().to_string(), read_objects);

        let find_files = Box::new(FindFiles);
        tools.insert(find_files.name().to_string(), find_files);

        let structure = Box::new(Structure);
        tools.insert(structure.name().to_string(), structure);

        let patch_file = Box::new(PatchFile);
        tools.insert(patch_file.name().to_string(), patch_file);

        let shell_exec = Box::new(crate::domain::tools::ShellExec);
        tools.insert(shell_exec.name().to_string(), shell_exec);

        let finish = Box::new(crate::domain::tools::Finish);
        tools.insert(finish.name().to_string(), finish);

        Self { tools }
    }
}

impl Default for GeneralToolset {
    fn default() -> Self {
        Self::new()
    }
}

impl Toolset for GeneralToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
