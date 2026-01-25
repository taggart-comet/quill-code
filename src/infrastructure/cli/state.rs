use crate::domain::tools::FileChange;
use crate::infrastructure::app_bus::{LocalModelInfo, OpenAiModelInfo};
use crate::infrastructure::cli::loading_bar::LoadingBar;
use ratatui_textarea::TextArea;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const INPUT_MIN_HEIGHT: usize = 3;
pub const INPUT_MAX_HEIGHT: usize = 6;
pub const PROGRESS_HISTORY_LIMIT: usize = 200;
pub const MAIN_BODY_SCROLL_STEP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Info,
    Success,
    Error,
    Cancelled,
}

pub const REQUEST_STATUS_DISPLAY_DURATION: Duration = Duration::from_secs(2);

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
    pub status: crate::infrastructure::app_bus::RequestStatus,
    pub finished_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ProgressEntry {
    pub text: String,
    pub kind: ProgressKind,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct FileChangesDisplay {
    pub request_id: i64,
    pub changes: Vec<FileChange>,
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
    },
    BraveApiKeyPrompt {
        web_search_enabled: bool,
        behavior_trees: bool,
        openai_tracing: bool,
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
    },
}

pub struct UiState {
    pub input: TextArea<'static>,
    pub header_title: Option<String>,
    pub progress: VecDeque<ProgressEntry>,
    pub active_progress: Option<usize>,
    pub current_model: String,
    pub mode: UiMode,
    pub popup_input: Option<PopupInput>,
    pub main_body_scroll: usize,
    pub models: ModelsCache,
    pub settings: SettingsCache,
    pub loading_bar: LoadingBar,
    pub openai_fetch_pending: bool,
    pub request_in_flight: Option<RequestIndicator>,
    pub request_status: Option<RequestStatusDisplay>,
    pub request_progress: Option<String>,
    pub file_changes: Option<FileChangesDisplay>,
    pub should_quit: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            input: TextArea::default(),
            header_title: None,
            progress: VecDeque::new(),
            active_progress: None,
            current_model: "unknown".to_string(),
            mode: UiMode::Normal,
            popup_input: None,
            main_body_scroll: 0,
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
            },
            loading_bar: LoadingBar::new(),
            openai_fetch_pending: false,
            request_in_flight: None,
            request_status: None,
            request_progress: None,
            file_changes: None,
            should_quit: false,
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
        if self.main_body_scroll > 0 {
            let added_lines = entry.text.lines().count().max(1);
            self.main_body_scroll = self.main_body_scroll.saturating_add(added_lines);
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
}
