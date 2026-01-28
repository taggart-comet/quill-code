use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentModeType {
    Build,         // Full toolset for implementation
    Plan,          // Read-only toolset for planning
    BuildFromPlan, // Auto-triggered: works through TODO list items sequentially
}

impl AgentModeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentModeType::Build => "build",
            AgentModeType::Plan => "plan",
            AgentModeType::BuildFromPlan => "build_from_plan",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "plan" | "PLAN" | "Plan" => AgentModeType::Plan,
            "build" | "BUILD" | "Build" => AgentModeType::Build,
            "build_from_plan" | "BUILD_FROM_PLAN" => AgentModeType::BuildFromPlan,
            _ => AgentModeType::Build,
        }
    }
}

impl Default for AgentModeType {
    fn default() -> Self {
        AgentModeType::Build
    }
}
