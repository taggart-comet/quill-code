use crate::infrastructure::app_bus::{EventBus, UiToAgentEvent};
use crate::infrastructure::cli::repl::refresh_settings_from_db;
use crate::infrastructure::cli::state::{LoadStatus, PopupInput, PopupState, UiMode, UiState};
use crate::repository::UserSettingsRepository;
use rusqlite::Connection;

pub fn api_key_missing(conn: &Connection) -> Result<bool, String> {
    let settings_repo = UserSettingsRepository::new(conn);
    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
    Ok(settings
        .openai_api_key
        .as_ref()
        .map_or(true, |key| key.trim().is_empty()))
}

pub fn begin_prompt(state: &mut UiState, enable_tracing: bool) {
    state.popup_input = Some(PopupInput::new());
    state.mode = UiMode::Popup(PopupState::OpenAiApiKeyPrompt { enable_tracing });
}

pub fn submit_key(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    enable_tracing: bool,
) -> Result<(), String> {
    let api_key = state
        .popup_input
        .as_ref()
        .map(|input| input.text.clone())
        .unwrap_or_default();
    if api_key.trim().is_empty() {
        return Ok(());
    }
    let _ = bus
        .ui_to_agent_tx
        .send(UiToAgentEvent::SettingsUpdateEvent {
            model: None,
            openai_api_key: Some(api_key),
            use_behavior_trees: None,
            openai_tracing_enabled: if enable_tracing { Some(true) } else { None },
            web_search_enabled: None,
            brave_api_key: None,
        });
    refresh_settings_from_db(conn, state)?;
    if enable_tracing {
        state.mode = UiMode::Normal;
    } else {
        state.models.openai_available_status = LoadStatus::Loading;
        state.openai_fetch_pending = true;
        state.mode = UiMode::Popup(PopupState::OpenAiAvailable {
            selected: 0,
            filter: String::new(),
            cursor: 0,
        });
    }
    state.popup_input = None;
    Ok(())
}
