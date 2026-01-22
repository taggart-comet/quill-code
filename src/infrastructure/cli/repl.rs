use crate::domain::ModelType;
use crate::infrastructure::app_bus::{
    AgentToUiEvent, EventBus, ModelSelection, RequestStatus, StepPhase,
};
use crate::infrastructure::cli::state::{
    LoadStatus, PopupState, ProgressEntry, ProgressKind, RequestIndicator, RequestStatusDisplay,
    UiMode, UiState,
};
use crate::infrastructure::db;
use crate::infrastructure::init::get_current_model_name;
use crate::infrastructure::inference::openai::OpenAIEngine;

use crate::repository::{MetaRepository, ModelsRepository, UserSettingsRepository};
use crossterm::event::{self, Event as CrosstermEvent};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::Terminal;
use rusqlite::Connection;
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

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;
    terminal.clear().map_err(|e| e.to_string())?;

    let result = run_loop(&mut terminal, &bus, &conn, &mut state);

    disable_raw_mode().map_err(|e| e.to_string())?;
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
    conn: &Connection,
    state: &mut UiState,
) -> Result<(), String> {
    let tick_rate = Duration::from_millis(50);

    loop {
        if state.should_quit {
            break;
        }

        if event::poll(tick_rate).map_err(|e| e.to_string())? {
            if let CrosstermEvent::Key(key) = event::read().map_err(|e| e.to_string())? {
                crate::infrastructure::cli::controls::handle_key_event(bus, conn, state, key)?;
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
    conn: &Connection,
    state: &mut UiState,
    event: AgentToUiEvent,
) -> Result<(), String> {
    match event {
        AgentToUiEvent::SessionStartedEvent { title } => {
            state.header_title = Some(title);
        }
        AgentToUiEvent::RequestStartedEvent { request_id, label } => {
            state.request_in_flight = Some(RequestIndicator {
                request_id,
                label,
                started_at: std::time::Instant::now(),
            });
            state.request_status = None;
        }
        AgentToUiEvent::ProgressEvent {
            step_name,
            phase,
            summary,
        } => {
            let prefix = match phase {
                StepPhase::Before => "<Before>",
                StepPhase::After => "<After>",
            };
            let text = format!("{} {} - {}", prefix, step_name, summary);
            let active = matches!(phase, StepPhase::Before);
            if matches!(phase, StepPhase::After) {
                for entry in state.progress.iter_mut() {
                    entry.active = false;
                }
                state.active_progress = None;
            }
            state.push_progress(ProgressEntry {
                text,
                kind: ProgressKind::Info,
                active,
            });
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
            if let Some(summary) = summary {
                state.push_progress(ProgressEntry {
                    text: summary,
                    kind: finish_kind,
                    active: false,
                });
            }
            if let Some(message) = final_message {
                for line in message.lines() {
                    if !line.trim().is_empty() {
                        state.push_progress(ProgressEntry {
                            text: line.to_string(),
                            kind: ProgressKind::Info,
                            active: false,
                        });
                    }
                }
            }
        }
        AgentToUiEvent::FileChangesEvent {
            request_id,
            changes,
        } => {
            state.push_progress(ProgressEntry {
                text: format!("<Changed> Files changed in request {}:", request_id),
                kind: ProgressKind::Info,
                active: false,
            });
            for change in changes {
                state.push_progress(ProgressEntry {
                    text: format!(
                        "{}: +{} -{}",
                        change.path, change.added_lines, change.deleted_lines
                    ),
                    kind: ProgressKind::Info,
                    active: false,
                });
            }
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
        } => {
            state.mode = UiMode::Popup(PopupState::PermissionPrompt {
                request_id,
                tool_name,
                command,
                paths,
                scope,
                selected: 0,
            });
        }
    }

    Ok(())
}

pub(crate) fn refresh_models_from_db(conn: &Connection, state: &mut UiState) -> Result<(), String> {
    let models_repo = ModelsRepository::new(conn);
    let models = models_repo
        .find_by_type(ModelType::OpenAI)
        .map_err(|e| e.to_string())?;
    state.models.openai = models
        .into_iter()
        .filter_map(|model| {
            model
                .model_name
                .map(|name| crate::infrastructure::app_bus::OpenAiModelInfo {
                    _id: model.id,
                    name,
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

pub(crate) fn refresh_settings_from_db(conn: &Connection, state: &mut UiState) -> Result<(), String> {
    let settings_repo = UserSettingsRepository::new(conn);
    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
    state.settings.use_behavior_trees = settings.use_behavior_trees;
    state.settings.openai_tracing_enabled = settings.openai_tracing_enabled;
    state.settings.web_search_enabled = settings.web_search_enabled;
    state.settings.status = LoadStatus::Loaded;
    state.current_model = get_current_model_name(conn).unwrap_or_else(|_| "unknown".to_string());
    Ok(())
}

pub(crate) fn update_settings_in_db(
    conn: &Connection,
    use_behavior_trees: bool,
    openai_tracing: bool,
    web_search: bool,
) -> Result<(), String> {
    let settings_repo = UserSettingsRepository::new(conn);
    settings_repo
        .update_use_behavior_trees(use_behavior_trees)
        .map_err(|e| e.to_string())?;
    settings_repo
        .update_openai_tracing_enabled(openai_tracing)
        .map_err(|e| e.to_string())?;
    settings_repo
        .update_web_search_enabled(web_search)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn update_model_selection_in_db(
    conn: &Connection,
    selection: &ModelSelection,
) -> Result<(), String> {
    let models_repo = ModelsRepository::new(conn);
    let meta_repo = MetaRepository::new(conn);
    let settings_repo = UserSettingsRepository::new(conn);

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
                Some(model) => model.id,
                None => {
                    models_repo
                        .create(ModelType::Local, None, Some(&gguf_path), None)
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
            let model_id = match existing_models
                .iter()
                .find(|model| model.model_name.as_deref() == Some(model_name))
            {
                Some(model) => model.id,
                None => {
                    models_repo
                        .create(ModelType::OpenAI, None, None, Some(model_name))
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

pub(crate) fn fetch_openai_available_models(conn: &Connection, state: &mut UiState) -> Result<(), String> {
    let settings_repo = UserSettingsRepository::new(conn);
    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
    let api_key = match settings.openai_api_key {
        Some(key) => key,
        None => {
            state.models.openai_available =
                vec!["OpenAI API key is required. Please set it in settings.".to_string()];
            state.models.openai_available_status = LoadStatus::Loaded;
            return Ok(());
        }
    };

    state.models.openai_available_status = LoadStatus::Loading;
    match OpenAIEngine::new_general(&api_key, "gpt-4") {
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
    use crate::infrastructure::app_bus::{LocalModelInfo, ModelSelection, OpenAiModelInfo};
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
        format!("drastis-test-{}", nanos)
    }

    fn cleanup_db(app_name: &str) {
        if let Some(dirs) = ProjectDirs::from("", "", app_name) {
            let _ = fs::remove_dir_all(dirs.data_dir());
        }
    }

    fn with_test_db<F>(f: F)
    where
        F: FnOnce(std::sync::Arc<Connection>, String),
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
            let settings_repo = UserSettingsRepository::new(&conn);
            let settings = settings_repo.get_current().expect("settings");
            let model_id = settings.current_model_id.expect("model id");

            let meta_repo = MetaRepository::new(&conn);
            assert_eq!(meta_repo.get_last_used_model_id().expect("meta"), Some(model_id));

            let models_repo = ModelsRepository::new(&conn);
            let model = models_repo.find_by_id(model_id).expect("model row").expect("model");
            assert_eq!(model.model_type, ModelType::OpenAI);
            assert_eq!(model.model_name.as_deref(), Some("gpt-4o"));
        });
    }

    #[test]
    fn update_model_selection_local_persists_model() {
        with_test_db(|conn, _| {
            let temp_path = create_temp_file("drastis-local-model");
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

            let settings_repo = UserSettingsRepository::new(&conn);
            let settings = settings_repo.get_current().expect("settings");
            let model_id = settings.current_model_id.expect("model id");

            let meta_repo = MetaRepository::new(&conn);
            assert_eq!(meta_repo.get_last_used_model_id().expect("meta"), Some(model_id));

            let models_repo = ModelsRepository::new(&conn);
            let model = models_repo.find_by_id(model_id).expect("model row").expect("model");
            assert_eq!(model.model_type, ModelType::Local);
            assert_eq!(model.gguf_file_path.as_deref(), Some(canonical.as_str()));
        });
    }
}
