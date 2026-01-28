use crate::domain::permissions::PermissionDecision;
use crate::domain::tools::FileChange;
use crate::domain::AgentModeType;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crate::domain::todo::TodoItem;

#[derive(Debug, Clone)]
pub enum StepPhase {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestStatus {
    Success,
    Failure,
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ModelSelection {
    LocalPath(String),
    OpenAiModel(String),
}

#[derive(Debug, Clone)]
pub struct LocalModelInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiModelInfo {
    pub _id: i64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ImageAttachment {
    pub data_url: String, // "data:image/png;base64,..."
}

#[derive(Debug, Clone)]
pub enum UiToAgentEvent {
    RequestEvent {
        prompt: String,
        images: Vec<ImageAttachment>,
        mode: AgentModeType, // NEW: Pass mode to agent
        session_id: Option<i64>,
    },
    SessionContinueEvent {
        session_id: i64,
    },
    PermissionUpdateEvent {
        request_id: u64,
        decision: PermissionDecision,
    },
    ShutdownEvent,
    SettingsUpdateEvent {
        model: Option<ModelSelection>,
        openai_api_key: Option<String>,
        use_behavior_trees: Option<bool>,
        openai_tracing_enabled: Option<bool>,
        web_search_enabled: Option<bool>,
        brave_api_key: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum AgentToUiEvent {
    SessionStartedEvent {
        title: String,
    },
    RequestStartedEvent {
        request_id: u64,
        label: String,
        prompt: String,
    },
    ProgressEvent {
        step_name: String,
        phase: StepPhase,
        summary: String,
    },
    PermissionRequestEvent {
        request_id: u64,
        tool_name: String,
        command: Option<String>,
        paths: Vec<String>,
        scope: String,
    },
    RequestFinishedEvent {
        request_id: u64,
        status: RequestStatus,
        summary: Option<String>,
        final_message: Option<String>,
    },
    SettingsSnapshot,
    FileChangesEvent {
        request_id: i64,
        changes: Vec<FileChange>,
    },
    TodoListUpdateEvent {
        items: Vec<TodoItem>,
    },
}

#[derive(Debug, Clone)]
pub struct PermissionUpdate {
    pub request_id: u64,
    pub decision: PermissionDecision,
}

#[derive(Clone, Debug)]
pub struct EventBus {
    pub ui_to_agent_tx: Sender<UiToAgentEvent>,
    pub ui_to_agent_rx: Receiver<UiToAgentEvent>,
    pub agent_to_ui_tx: Sender<AgentToUiEvent>,
    pub agent_to_ui_rx: Receiver<AgentToUiEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (ui_to_agent_tx, ui_to_agent_rx) = unbounded();
        let (agent_to_ui_tx, agent_to_ui_rx) = unbounded();
        Self {
            ui_to_agent_tx,
            ui_to_agent_rx,
            agent_to_ui_tx,
            agent_to_ui_rx,
        }
    }
}
