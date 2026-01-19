use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use std::collections::HashMap;

/// General toolset containing read-only and utility tools
pub struct EditToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl EditToolset {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        let patch_files = Box::new(PatchFiles::new());
        tools.insert(patch_files.name().to_string(), patch_files);

        Self { tools }
    }
}

impl Toolset for EditToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
