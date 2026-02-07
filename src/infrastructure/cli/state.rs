use crate::domain::tools::FileChange;
use crate::domain::AgentModeType;
use crate::infrastructure::cli::loading_bar::LoadingBar;
use crate::infrastructure::event_bus::{LocalModelInfo, OpenAiModelInfo};
use ratatui_textarea::TextArea;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AttachedImage {
    /// Base64-encoded image data
    pub data: String,
    /// MIME type (e.g., "image/png", "image/jpeg")
    pub mime_type: String,
    /// Human-readable size (e.g., "234 KB")
    pub size: String,
    /// Timestamp when attached
    pub attached_at: Instant,
}

impl AttachedImage {
    pub fn new(data: String, mime_type: String, size: String) -> Self {
        Self {
            data,
            mime_type,
            size,
            attached_at: Instant::now(),
        }
    }

    pub fn to_data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime_type, self.data)
    }
}

pub const INPUT_MIN_HEIGHT: usize = 1;
pub const INPUT_MAX_HEIGHT: usize = 5;
pub const PROGRESS_HISTORY_LIMIT: usize = 200;
pub const MAIN_BODY_SCROLL_STEP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Info,
    Success,
    Error,
    Cancelled,
    UserMessage,
    UserMessageSuccess,
    UserMessageError,
    UserMessageCancelled,
}

pub const REQUEST_STATUS_DISPLAY_DURATION: Duration = Duration::from_secs(2);
pub const MIN_PROGRESS_DISPLAY_MS: u128 = 1000; // 1 second minimum display time

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RequestIndicator {
    pub request_id: u64,
    pub label: String,
    pub started_at: Instant,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RequestStatusDisplay {
    pub request_id: u64,
    pub status: crate::infrastructure::event_bus::RequestStatus,
    pub finished_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ProgressEntry {
    pub text: String,
    pub kind: ProgressKind,
    pub active: bool,
    pub request_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct FileChangesDisplay {
    pub request_id: i64,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone)]
pub struct TodoListDisplay {
    pub items: Vec<TodoItemDisplay>,
}

#[derive(Debug, Clone)]
pub struct TodoItemDisplay {
    pub title: String,
    pub description: String,
    pub status: String, // "pending", "in_progress", "completed"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    Unknown,
    Loading,
    Loaded,
}

#[derive(Debug, Clone)]
pub struct ModelsCache {
    pub status: LoadStatus,
    pub local: Vec<LocalModelInfo>,
    pub openai: Vec<OpenAiModelInfo>,
    pub openai_available: Vec<String>,
    pub openai_available_status: LoadStatus,
}

#[derive(Debug, Clone)]
pub struct SettingsCache {
    pub status: LoadStatus,
    pub use_behavior_trees: bool,
    pub openai_tracing_enabled: bool,
    pub web_search_enabled: bool,
    pub max_tool_calls_per_request: i32,
}

#[derive(Debug, Clone)]
pub struct PopupInput {
    pub text: String,
    pub cursor: usize,
}

impl PopupInput {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiMode {
    Normal,
    CommandsMenu { selected: usize },
    Popup(PopupState),
    FileChangesReview {
        selected_file: usize,
        view_mode: FileChangesViewMode,
        scroll_offset: usize,
    },
    TodoListReview {
        selected_item: usize,
        view_mode: TodoListViewMode,
        scroll_offset: usize,
    },
}

#[derive(Debug, Clone)]
pub enum FileChangesViewMode {
    FileList,
    UnifiedDiff,
}

#[derive(Debug, Clone)]
pub enum TodoListViewMode {
    ItemList,
    ItemDetail,
}

#[derive(Debug, Clone)]
pub enum PopupState {
    ModelSelect {
        selected: usize,
    },
    OpenAiAvailable {
        selected: usize,
        filter: String,
        cursor: usize,
    },
    SettingsToggle {
        selected: usize,
        behavior_trees: bool,
        openai_tracing: bool,
        web_search: bool,
        max_tool_calls: i32,
    },
    BraveApiKeyPrompt {
        web_search_enabled: bool,
        behavior_trees: bool,
        openai_tracing: bool,
        max_tool_calls: i32,
    },
    OpenAiApiKeyPrompt {
        enable_tracing: bool,
    },
    ModeSelect {
        selected: usize,
    },
    PermissionPrompt {
        request_id: u64,
        tool_name: String,
        command: Option<String>,
        paths: Vec<String>,
        scope: String,
        selected: usize,
        is_read_only: bool,
    },
    ContinueSelect {
        sessions: Vec<SessionPreview>,
        selected: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SessionPreview {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

pub fn format_relative_time(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let delta = now.saturating_sub(epoch_secs);
    if delta < 60 {
        return "just now".to_string();
    }
    let mins = delta / 60;
    if mins < 60 {
        return format!("{} min ago", mins);
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{} hr ago", hours);
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{} days ago", days);
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{} weeks ago", weeks);
    }
    let months = days / 30;
    format!("{} months ago", months)
}

pub struct UiState {
    pub input: TextArea<'static>,
    pub header_title: Option<String>,
    pub progress: VecDeque<ProgressEntry>,
    pub active_progress: Option<usize>,
    pub current_model: String,
    pub current_model_type: Option<crate::domain::ModelType>,
    pub mode: UiMode,
    pub popup_input: Option<PopupInput>,
    pub main_body_scroll: usize,
    pub main_body_follow: bool,
    pub models: ModelsCache,
    pub settings: SettingsCache,
    pub loading_bar: LoadingBar,
    pub openai_fetch_pending: bool,
    pub request_in_flight: Option<RequestIndicator>,
    pub request_status: Option<RequestStatusDisplay>,
    pub request_progress: Option<String>,
    pub last_progress_update: Option<Instant>,
    pub file_changes: Option<FileChangesDisplay>,
    pub agent_mode: AgentModeType,
    pub todo_list: Option<TodoListDisplay>,
    pub should_quit: bool,
    pub attached_images: Vec<AttachedImage>,
    pub session_id: Option<i64>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            input: TextArea::default(),
            header_title: None,
            progress: VecDeque::new(),
            active_progress: None,
            current_model: "unknown".to_string(),
            current_model_type: None,
            mode: UiMode::Normal,
            popup_input: None,
            main_body_scroll: 0,
            main_body_follow: true,
            models: ModelsCache {
                status: LoadStatus::Unknown,
                local: Vec::new(),
                openai: Vec::new(),
                openai_available: Vec::new(),
                openai_available_status: LoadStatus::Unknown,
            },
            settings: SettingsCache {
                status: LoadStatus::Unknown,
                use_behavior_trees: false,
                openai_tracing_enabled: false,
                web_search_enabled: false,
                max_tool_calls_per_request: 50,
            },
            loading_bar: LoadingBar::new(),
            openai_fetch_pending: false,
            request_in_flight: None,
            request_status: None,
            request_progress: None,
            last_progress_update: None,
            file_changes: None,
            agent_mode: AgentModeType::Build, // Default to build mode
            todo_list: None,
            should_quit: false,
            attached_images: Vec::new(),
            session_id: None,
        }
    }

    pub fn input_line_count(&self) -> usize {
        let count = self.input.lines().len().max(1);
        count.max(INPUT_MIN_HEIGHT)
    }

    pub fn push_progress(&mut self, entry: ProgressEntry) {
        if self.progress.len() >= PROGRESS_HISTORY_LIMIT {
            self.progress.pop_front();
        }
        if entry.active {
            for existing in self.progress.iter_mut() {
                existing.active = false;
            }
            self.active_progress = Some(self.progress.len());
        }
        // Simplified scroll logic: only auto-scroll when following
        // When user has scrolled up, their viewport stays at that position
        // as new messages arrive (standard chat behavior)
        if self.main_body_follow {
            self.main_body_scroll = 0;
        }
        self.progress.push_back(entry);
    }

    pub fn clear_expired_request_status(&mut self) {
        if let Some(status) = &self.request_status {
            if status.finished_at.elapsed() >= REQUEST_STATUS_DISPLAY_DURATION {
                self.request_status = None;
            }
        }
    }

    pub fn clear_input_and_attachments(&mut self) {
        self.input = TextArea::default();
        self.attached_images.clear();
    }

    pub fn can_attach_images(&self) -> bool {
        // All OpenAI models support vision
        matches!(
            self.current_model_type,
            Some(crate::domain::ModelType::OpenAI)
        )
    }
}