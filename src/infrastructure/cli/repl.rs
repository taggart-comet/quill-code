use crate::domain::{AgentModeType, ModelAuthType, ModelType};
use crate::infrastructure::cli::state::{
    FileChangesDisplay, LoadStatus, PopupState, ProgressEntry, ProgressKind, RequestIndicator,
    RequestStatusDisplay, UiMode, UiState,
};
use crate::infrastructure::db::{self, DbPool};
use crate::infrastructure::event_bus::{
    AgentToUiEvent, EventBus, ModelSelection, RequestStatus, StepPhase,
};
use crate::infrastructure::inference::openai::OpenAIEngine;

use crate::repository::{MetaRepository, ModelsRepository, UserSettingsRepository};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as CrosstermEvent,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

pub fn run(bus: EventBus, app_name: String) -> Result<(), String> {
    let mut state = UiState::new();
    let conn = db::init_db(&app_name).map_err(|e| e.to_string())?;
    refresh_settings_from_db(&conn, &mut state)?;

    let mut stdout = io::stdout();
    enable_raw_mode().map_err(|e| e.to_string())?;
    stdout
        .execute(EnterAlternateScreen)
        .map_err(|e| e.to_string())?;
    stdout
        .execute(EnableBracketedPaste)
        .map_err(|e| e.to_string())?;
    stdout
        .execute(EnableMouseCapture)
        .map_err(|e| e.to_string())?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;
    terminal.clear().map_err(|e| e.to_string())?;
    terminal
        .backend_mut()
        .execute(EnableMouseCapture)
        .map_err(|e| e.to_string())?;

    let result = run_loop(&mut terminal, &bus, &conn, &mut state);

    disable_raw_mode().map_err(|e| e.to_string())?;
    terminal
        .backend_mut()
        .execute(DisableBracketedPaste)
        .map_err(|e| e.to_string())?;
    terminal
        .backend_mut()
        .execute(DisableMouseCapture)
        .map_err(|e| e.to_string())?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .map_err(|e| e.to_string())?;
    terminal.show_cursor().map_err(|e| e.to_string())?;

    result
}

fn run_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
) -> Result<(), String> {
    let tick_rate = Duration::from_millis(50);

    loop {
        if state.should_quit {
            break;
        }

        if event::poll(tick_rate).map_err(|e| e.to_string())? {
            let evt = event::read().map_err(|e| e.to_string())?;
            match evt {
                CrosstermEvent::Key(key) => {
                    // Log Cmd+V attempts
                    if key.code == crossterm::event::KeyCode::Char('v')
                        && (key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::SUPER)
                            || key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL))
                    {
                        log::info!(
                            "Cmd+V / Ctrl+V key event received (modifiers: {:?})",
                            key.modifiers
                        );
                    }
                    crate::infrastructure::cli::controls::handle_key_event(bus, conn, state, key)?;
                }
                CrosstermEvent::Mouse(mouse) => {
                    crate::infrastructure::cli::controls::handle_mouse_event(state, mouse);
                }
                CrosstermEvent::Paste(data) => {
                    log::info!("!!!! PASTE EVENT RECEIVED with {} bytes !!!!", data.len());
                    crate::infrastructure::cli::controls::handle_paste_event(state, data)?;
                }
                CrosstermEvent::Resize(_, _) => {
                    log::debug!("Resize event");
                }
                CrosstermEvent::FocusGained => {
                    log::debug!("Focus gained");
                }
                CrosstermEvent::FocusLost => {
                    log::debug!("Focus lost");
                }
            }
        }

        while let Ok(event) = bus.agent_to_ui_rx.try_recv() {
            handle_agent_event(conn, state, event)?;
        }

        state.clear_expired_request_status();

        terminal
            .draw(|frame| crate::infrastructure::cli::views::main_view::render(frame, state))
            .map_err(|e| e.to_string())?;

        if state.openai_fetch_pending {
            state.openai_fetch_pending = false;
            fetch_openai_available_models(conn, state)?;
        }
    }

    Ok(())
}

fn handle_agent_event(
    conn: &DbPool,
    state: &mut UiState,
    event: AgentToUiEvent,
) -> Result<(), String> {
    match event {
        AgentToUiEvent::SessionStartedEvent { title } => {
            state.header_title = Some(title);
        }
        AgentToUiEvent::RequestStartedEvent {
            request_id,
            label,
            prompt,
        } => {
            state.request_in_flight = Some(RequestIndicator {
                request_id,
                label,
                started_at: std::time::Instant::now(),
            });
            state.request_status = None;
            state.request_progress = None;
            state.last_progress_update = None; // Reset timer for new request
            state.request_tool_calls = Some(0);
            state.file_changes = None;

            // Display the user's message
            state.push_progress(ProgressEntry {
                text: prompt,
                kind: ProgressKind::UserMessage,
                active: false,
                request_id: Some(request_id),
            });
        }
        AgentToUiEvent::ProgressEvent {
            step_name,
            phase,
            summary,
        } => {
            if matches!(phase, StepPhase::Before) && step_name != "inference" {
                if let Some(current) = state.request_tool_calls.as_mut() {
                    *current = current.saturating_add(1);
                } else {
                    state.request_tool_calls = Some(1);
                }
            }
            if matches!(phase, StepPhase::Before) {
                // Check if enough time has passed since the last progress update
                let can_update = state
                    .last_progress_update
                    .map(|last| {
                        std::time::Instant::now().duration_since(last).as_millis()
                            >= crate::infrastructure::cli::state::MIN_PROGRESS_DISPLAY_MS
                    })
                    .unwrap_or(true); // Update immediately if no previous update

                if can_update {
                    state.request_progress = Some(summary.clone());
                    state.last_progress_update = Some(std::time::Instant::now());
                }
                // Otherwise skip this update - too soon since last one
            } else {
                state.request_progress = None;
            }
        }
        AgentToUiEvent::RequestFinishedEvent {
            request_id,
            status,
            summary,
            final_message,
        } => {
            let finish_kind = match status {
                RequestStatus::Success => ProgressKind::Success,
                RequestStatus::Failure => ProgressKind::Error,
                RequestStatus::Cancelled => ProgressKind::Cancelled,
            };
            let matches_active = state
                .request_in_flight
                .as_ref()
                .map(|active| active.request_id == request_id)
                .unwrap_or(false);
            if matches_active {
                state.request_in_flight = None;
            }
            if request_id != 0 && (matches_active || state.request_in_flight.is_none()) {
                state.request_status = Some(RequestStatusDisplay {
                    request_id,
                    status,
                    finished_at: std::time::Instant::now(),
                });
            }
            state.request_progress = None;
            state.last_progress_update = None; // Reset timer when request finishes
            state.request_tool_calls = None;

            // Update the user message color based on request status
            for entry in state.progress.iter_mut() {
                if entry.request_id == Some(request_id) && entry.kind == ProgressKind::UserMessage {
                    entry.kind = match status {
                        RequestStatus::Success => ProgressKind::UserMessageSuccess,
                        RequestStatus::Failure => ProgressKind::UserMessageError,
                        RequestStatus::Cancelled => ProgressKind::UserMessageCancelled,
                    };
                }
            }

            // Show failures via summary; for successful requests, fallback to summary
            // when final_message is missing/empty (common when resuming older sessions).
            if let Some(summary_text) = summary {
                if status != RequestStatus::Success {
                    state.push_progress(ProgressEntry {
                        text: summary_text,
                        kind: finish_kind,
                        active: false,
                        request_id: None,
                    });
                } else {
                    let final_is_empty = final_message
                        .as_ref()
                        .map(|m| m.trim().is_empty())
                        .unwrap_or(true);
                    if final_is_empty {
                        state.push_progress(ProgressEntry {
                            text: summary_text,
                            kind: ProgressKind::Info,
                            active: false,
                            request_id: None,
                        });
                    }
                }
            }
            if let Some(message) = final_message {
                for line in message.lines() {
                    if !line.trim().is_empty() {
                        state.push_progress(ProgressEntry {
                            text: line.to_string(),
                            kind: ProgressKind::Info,
                            active: false,
                            request_id: None,
                        });
                    }
                }
            }

            // Revert BuildFromPlan → Build when all TODO items are completed
            if state.agent_mode == AgentModeType::BuildFromPlan {
                let all_done = state
                    .todo_list
                    .as_ref()
                    .map(|list| {
                        !list.items.is_empty()
                            && list.items.iter().all(|item| item.status == "completed")
                    })
                    .unwrap_or(true);
                if all_done {
                    state.agent_mode = AgentModeType::Build;
                }
            }
        }
        AgentToUiEvent::FileChangesEvent {
            request_id,
            changes,
        } => {
            state.file_changes = Some(FileChangesDisplay {
                request_id,
                changes,
            });
        }
        AgentToUiEvent::SettingsSnapshot => {
            refresh_settings_from_db(conn, state)?;
        }
        AgentToUiEvent::PermissionRequestEvent {
            request_id,
            tool_name,
            command,
            paths,
            scope,
            is_read_only,
        } => {
            state.mode = UiMode::Popup(PopupState::PermissionPrompt {
                request_id,
                tool_name,
                command,
                paths,
                scope,
                selected: 0,
                is_read_only,
            });
        }
        AgentToUiEvent::TodoListUpdateEvent { items } => {
            use crate::infrastructure::cli::state::{TodoItemDisplay, TodoListDisplay};
            state.todo_list = Some(TodoListDisplay {
                items: items
                    .into_iter()
                    .map(|item| TodoItemDisplay {
                        title: item.title,
                        description: item.description,
                        status: item.status.as_str().to_string(),
                    })
                    .collect(),
            });
        }
    }

    Ok(())
}

pub(crate) fn refresh_models_from_db(conn: &DbPool, state: &mut UiState) -> Result<(), String> {
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))?;
    let models_repo = ModelsRepository::new(&*conn_guard);
    let models = models_repo
        .find_by_type(ModelType::OpenAI)
        .map_err(|e| e.to_string())?;
    state.models.openai = models
        .into_iter()
        .filter_map(|model| {
            model
                .model_name
                .map(|name| crate::infrastructure::event_bus::OpenAiModelInfo {
                    _id: model.id,
                    name,
                    auth_type: model.auth_type.as_str().to_string(),
                })
        })
        .collect();

    state.models.openai_available.clear();
    state.models.openai_available_status = LoadStatus::Unknown;

    state.models.local =
        crate::infrastructure::init::list_local_models().map_err(|e| e.to_string())?;
    state.models.status = LoadStatus::Loaded;
    Ok(())
}

pub(crate) fn refresh_settings_from_db(conn: &DbPool, state: &mut UiState) -> Result<(), String> {
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))?;
    let settings_repo = UserSettingsRepository::new(&*conn_guard);
    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
    state.settings.use_behavior_trees = settings.use_behavior_trees;
    state.settings.openai_tracing_enabled = settings.openai_tracing_enabled;
    state.settings.web_search_enabled = settings.web_search_enabled;
    state.settings.max_tool_calls_per_request = settings.max_tool_calls_per_request;
    state.settings.auth_method = crate::domain::AuthMethod::from_str(&settings.auth_method);
    state.settings.oauth_token_expiry = settings.oauth_token_expiry;
    state.settings.status = LoadStatus::Loaded;

    // Get both model name and type
    match crate::infrastructure::init::get_current_model_info(conn) {
        Ok((name, model_type)) => {
            state.current_model = name;
            state.current_model_type = Some(model_type);
        }
        Err(_) => {
            state.current_model = "unknown".to_string();
            state.current_model_type = None;
        }
    }

    Ok(())
}

pub(crate) fn update_settings_in_db(
    conn: &DbPool,
    use_behavior_trees: bool,
    openai_tracing: bool,
    web_search: bool,
    max_tool_calls: i32,
) -> Result<(), String> {
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))?;
    let settings_repo = UserSettingsRepository::new(&*conn_guard);
    settings_repo
        .update_use_behavior_trees(use_behavior_trees)
        .map_err(|e| e.to_string())?;
    settings_repo
        .update_openai_tracing_enabled(openai_tracing)
        .map_err(|e| e.to_string())?;
    settings_repo
        .update_web_search_enabled(web_search)
        .map_err(|e| e.to_string())?;
    settings_repo
        .update_max_tool_calls_per_request(max_tool_calls)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn update_model_selection_in_db(
    conn: &DbPool,
    selection: &ModelSelection,
) -> Result<(), String> {
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))?;
    let models_repo = ModelsRepository::new(&*conn_guard);
    let meta_repo = MetaRepository::new(&*conn_guard);
    let settings_repo = UserSettingsRepository::new(&*conn_guard);

    match selection {
        ModelSelection::LocalPath(path) => {
            let path_buf = std::path::PathBuf::from(path);
            if !path_buf.exists() {
                return Err(format!("Model file not found: {}", path));
            }
            let canonical_path = path_buf.canonicalize().unwrap_or_else(|_| path_buf.clone());
            let gguf_path = canonical_path.to_string_lossy().to_string();

            let existing_models = models_repo
                .find_by_type(ModelType::Local)
                .map_err(|e| e.to_string())?;
            let existing_model = existing_models.into_iter().find(|model| {
                model.gguf_file_path.as_ref().map_or(false, |existing| {
                    let existing_abs = std::path::PathBuf::from(existing)
                        .canonicalize()
                        .unwrap_or_else(|_| std::path::PathBuf::from(existing));
                    existing_abs == canonical_path
                })
            });
            let model_id = match existing_model {
                Some(model) => {
                    if model.auth_type != ModelAuthType::Local {
                        models_repo
                            .update_auth_type(model.id, ModelAuthType::Local)
                            .map_err(|e| e.to_string())?;
                    }
                    model.id
                }
                None => {
                    models_repo
                        .create(
                            ModelType::Local,
                            Some(&gguf_path),
                            None,
                            ModelAuthType::Local,
                        )
                        .map_err(|e| e.to_string())?
                        .id
                }
            };

            meta_repo
                .set_last_used_model_id(model_id)
                .map_err(|e| e.to_string())?;
            settings_repo
                .update_current_model_id(Some(model_id))
                .map_err(|e| e.to_string())?;
        }
        ModelSelection::OpenAiModel(model_name) => {
            let existing_models = models_repo
                .find_by_type(ModelType::OpenAI)
                .map_err(|e| e.to_string())?;
            let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
            let auth_method = crate::domain::AuthMethod::from_str(&settings.auth_method);
            let auth_type = ModelAuthType::from_auth_method(&auth_method);
            let model_id = match existing_models
                .iter()
                .find(|model| model.model_name.as_deref() == Some(model_name))
            {
                Some(model) => model.id,
                None => {
                    models_repo
                        .create(ModelType::OpenAI, None, Some(model_name), auth_type)
                        .map_err(|e| e.to_string())?
                        .id
                }
            };

            meta_repo
                .set_last_used_model_id(model_id)
                .map_err(|e| e.to_string())?;
            settings_repo
                .update_current_model_id(Some(model_id))
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub(crate) fn fetch_openai_available_models(
    conn: &DbPool,
    state: &mut UiState,
) -> Result<(), String> {
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))?;
    let settings_repo = UserSettingsRepository::new(&*conn_guard);
    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;

    let auth_method = crate::domain::AuthMethod::from_str(&settings.auth_method);
    let auth_token = if auth_method == crate::domain::AuthMethod::OAuth {
        match (&settings.oauth_access_token, &settings.oauth_account_id) {
            (Some(token), account_id) => {
                crate::infrastructure::api_clients::openai::client::AuthToken::OAuth {
                    token: token.clone(),
                    account_id: account_id.clone(),
                }
            }
            _ => {
                state.models.openai_available =
                    vec!["OAuth token not found. Please re-authenticate.".to_string()];
                state.models.openai_available_status = LoadStatus::Loaded;
                return Ok(());
            }
        }
    } else {
        match &settings.openai_api_key {
            Some(key) if !key.trim().is_empty() => {
                crate::infrastructure::api_clients::openai::client::AuthToken::ApiKey(key.clone())
            }
            _ => {
                state.models.openai_available =
                    vec!["OpenAI API key is required. Please set it in settings.".to_string()];
                state.models.openai_available_status = LoadStatus::Loaded;
                return Ok(());
            }
        }
    };

    // OAuth tokens can't call /v1/models — return known Codex/ChatGPT models instead
    if auth_method == crate::domain::AuthMethod::OAuth {
        state.models.openai_available = vec![
            "gpt-5.3-codex-spark".to_string(),
            "gpt-5.3-codex".to_string(),
            "gpt-5.2-codex".to_string(),
            "gpt-5.2".to_string(),
            "gpt-5.1-codex-max".to_string(),
            "gpt-5.1-codex".to_string(),
            "gpt-5.1".to_string(),
            "gpt-5-codex".to_string(),
            "gpt-5-codex-mini".to_string(),
            "gpt-5".to_string(),
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "o3".to_string(),
            "o4-mini".to_string(),
        ];
        state.models.openai_available_status = LoadStatus::Loaded;
        return Ok(());
    }

    state.models.openai_available_status = LoadStatus::Loading;
    match OpenAIEngine::new_general(auth_token, "gpt-4") {
        Ok(openai) => match openai.fetch_available_models() {
            Ok(models) => {
                state.models.openai_available = models;
                state.models.openai_available_status = LoadStatus::Loaded;
            }
            Err(e) => {
                state.models.openai_available = vec![format!("OpenAI model fetch failed: {}", e)];
                state.models.openai_available_status = LoadStatus::Loaded;
            }
        },
        Err(e) => {
            state.models.openai_available = vec![format!("OpenAI init failed: {}", e)];
            state.models.openai_available_status = LoadStatus::Loaded;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::cli::helpers::{
        cursor_position, delete_next_char, delete_prev_char, insert_char, next_char_boundary,
        prev_char_boundary,
    };
    use crate::infrastructure::cli::views::main_view::{
        model_entries, openai_available_entries, openai_available_filtered, ModelEntry,
    };
    use crate::infrastructure::event_bus::{LocalModelInfo, ModelSelection, OpenAiModelInfo};
    use crate::repository::{MetaRepository, ModelsRepository, UserSettingsRepository};
    use directories::ProjectDirs;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_app_name() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("quillcode-test-{}", nanos)
    }

    fn cleanup_db(app_name: &str) {
        if let Some(dirs) = ProjectDirs::from("", "", app_name) {
            let _ = fs::remove_dir_all(dirs.data_dir());
        }
    }

    fn with_test_db<F>(f: F)
    where
        F: FnOnce(DbPool, String),
    {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        f(conn.clone(), app_name.clone());
        drop(conn);
        cleanup_db(&app_name);
    }

    fn create_temp_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let filename = format!("{}-{}.gguf", prefix, nanos);
        let path = std::env::temp_dir().join(filename);
        let _ = File::create(&path).expect("create temp file");
        path
    }

    fn remove_temp_file(path: &Path) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn openai_available_filtered_respects_status_and_filter() {
        let mut state = UiState::new();
        state.models.openai_available_status = LoadStatus::Loading;
        state.models.openai_available = vec!["gpt-4".to_string()];
        assert!(openai_available_filtered(&state, "gpt").is_empty());

        state.models.openai_available_status = LoadStatus::Loaded;
        state.models.openai_available = vec![
            "gpt-4".to_string(),
            "gpt-3.5-turbo".to_string(),
            "o1-mini".to_string(),
        ];
        let all = openai_available_filtered(&state, "");
        assert_eq!(all.len(), 3);
        let filtered = openai_available_filtered(&state, "mini");
        assert_eq!(filtered, vec!["o1-mini".to_string()]);
    }

    #[test]
    fn openai_available_entries_handles_loading_and_empty() {
        let mut state = UiState::new();
        state.models.openai_available_status = LoadStatus::Loading;
        let loading = openai_available_entries(&state, "");
        assert_eq!(loading, vec!["Loading models...".to_string()]);

        state.models.openai_available_status = LoadStatus::Loaded;
        state.models.openai_available = Vec::new();
        let empty = openai_available_entries(&state, "");
        assert_eq!(empty, vec!["No models available".to_string()]);

        state.models.openai_available = vec!["gpt-4".to_string()];
        let none = openai_available_entries(&state, "nope");
        assert_eq!(none, vec!["No matches found".to_string()]);
        let ok = openai_available_entries(&state, "gpt");
        assert_eq!(ok, vec!["gpt-4".to_string()]);
    }

    #[test]
    fn model_entries_reports_loading_and_select() {
        let mut state = UiState::new();
        state.models.status = LoadStatus::Loading;
        let loading = model_entries(&state);
        assert!(matches!(loading.first(), Some(ModelEntry::Loading)));

        state.models.status = LoadStatus::Loaded;
        state.models.local = Vec::new();
        state.models.openai = Vec::new();
        let select_only = model_entries(&state);
        assert_eq!(select_only.len(), 1);
        assert!(matches!(select_only[0], ModelEntry::OpenAiSelect));

        state.models.local = vec![LocalModelInfo {
            name: "local".to_string(),
            path: "/tmp/local.gguf".to_string(),
        }];
        state.models.openai = vec![OpenAiModelInfo {
            _id: 1,
            name: "gpt-4".to_string(),
            auth_type: "api_key".to_string(),
        }];
        let entries = model_entries(&state);
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], ModelEntry::Local { .. }));
        assert!(matches!(entries[1], ModelEntry::OpenAi { .. }));
        assert!(matches!(entries[2], ModelEntry::OpenAiSelect));
    }

    #[test]
    fn text_helpers_update_cursor_and_content() {
        let mut text = String::new();
        let mut cursor = 0;
        insert_char(&mut text, &mut cursor, 'a');
        assert_eq!(text, "a");
        assert_eq!(cursor, 1);

        text = "abc".to_string();
        cursor = 2;
        delete_prev_char(&mut text, &mut cursor);
        assert_eq!(text, "ac");
        assert_eq!(cursor, 1);

        text = "abc".to_string();
        cursor = 1;
        delete_next_char(&mut text, &mut cursor);
        assert_eq!(text, "ac");
        assert_eq!(cursor, 1);

        assert_eq!(prev_char_boundary("abc", 2), 1);
        assert_eq!(next_char_boundary("abc", 1), 2);

        let (col, line) = cursor_position("ab\ncde", 4);
        assert_eq!((col, line), (1, 1));
    }

    #[test]
    fn update_model_selection_openai_persists_model() {
        with_test_db(|conn, _| {
            update_model_selection_in_db(&conn, &ModelSelection::OpenAiModel("gpt-4o".to_string()))
                .expect("update model");
            let conn_guard = conn.get().expect("db conn");
            let settings_repo = UserSettingsRepository::new(&*conn_guard);
            let settings = settings_repo.get_current().expect("settings");
            let model_id = settings.current_model_id.expect("model id");

            let meta_repo = MetaRepository::new(&*conn_guard);
            assert_eq!(
                meta_repo.get_last_used_model_id().expect("meta"),
                Some(model_id)
            );

            let models_repo = ModelsRepository::new(&*conn_guard);
            let model = models_repo
                .find_by_id(model_id)
                .expect("model row")
                .expect("model");
            assert_eq!(model.model_type, ModelType::OpenAI);
            assert_eq!(model.model_name.as_deref(), Some("gpt-4o"));
        });
    }

    #[test]
    fn update_model_selection_local_persists_model() {
        with_test_db(|conn, _| {
            let temp_path = create_temp_file("quillcode-local-model");
            let result = update_model_selection_in_db(
                &conn,
                &ModelSelection::LocalPath(temp_path.to_string_lossy().to_string()),
            );
            result.expect("update model");
            let canonical = temp_path
                .canonicalize()
                .unwrap_or_else(|_| temp_path.clone())
                .to_string_lossy()
                .to_string();
            remove_temp_file(&temp_path);

            let conn_guard = conn.get().expect("db conn");
            let settings_repo = UserSettingsRepository::new(&*conn_guard);
            let settings = settings_repo.get_current().expect("settings");
            let model_id = settings.current_model_id.expect("model id");

            let meta_repo = MetaRepository::new(&*conn_guard);
            assert_eq!(
                meta_repo.get_last_used_model_id().expect("meta"),
                Some(model_id)
            );

            let models_repo = ModelsRepository::new(&*conn_guard);
            let model = models_repo
                .find_by_id(model_id)
                .expect("model row")
                .expect("model");
            assert_eq!(model.model_type, ModelType::Local);
            assert_eq!(model.gguf_file_path.as_deref(), Some(canonical.as_str()));
        });
    }
}