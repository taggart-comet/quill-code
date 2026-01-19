use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct ReadToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ReadToolset {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let discover_objects = Box::new(DiscoverObjects::new());
        tools.insert(discover_objects.name().to_string(), discover_objects);

        let read_objects = Box::new(ReadObjects::new());
        tools.insert(read_objects.name().to_string(), read_objects);

        let find_files = Box::new(FindFiles::new());
        tools.insert(find_files.name().to_string(), find_files);

        let structure = Box::new(Structure::new());
        tools.insert(structure.name().to_string(), structure);

        Self { tools }
    }
}

impl Toolset for ReadToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
