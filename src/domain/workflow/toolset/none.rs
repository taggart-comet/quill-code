use crate::domain::tools::*;
use crate::domain::workflow::toolset::Toolset;
use std::collections::HashMap;

pub struct NoneToolset {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl NoneToolset {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        Self { tools }
    }
}

impl Toolset for NoneToolset {
    fn tools(&self) -> &HashMap<String, Box<dyn Tool>> {
        &self.tools
    }
}
