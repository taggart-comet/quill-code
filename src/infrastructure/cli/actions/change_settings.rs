use crate::infrastructure::app_bus::{EventBus, UiToAgentEvent};
use crate::infrastructure::cli::repl::{refresh_settings_from_db, update_settings_in_db};
use crate::infrastructure::cli::state::{PopupState, UiMode, UiState};
use rusqlite::Connection;

pub fn open_settings_popup(conn: &Connection, state: &mut UiState) {
    let _ = refresh_settings_from_db(conn, state);
    state.mode = UiMode::Popup(PopupState::SettingsToggle {
        selected: 0,
        behavior_trees: state.settings.use_behavior_trees,
        openai_tracing: state.settings.openai_tracing_enabled,
        web_search: state.settings.web_search_enabled,
    });
}

pub fn submit_settings(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    behavior_trees: bool,
    openai_tracing: bool,
    web_search: bool,
) -> Result<(), String> {
    apply_settings(bus, conn, state, behavior_trees, openai_tracing, web_search)?;
    state.mode = UiMode::Normal;
    Ok(())
}

pub fn apply_settings(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    behavior_trees: bool,
    openai_tracing: bool,
    web_search: bool,
) -> Result<(), String> {
    update_settings_in_db(conn, behavior_trees, openai_tracing, web_search)?;
    let _ = bus
        .ui_to_agent_tx
        .send(UiToAgentEvent::SettingsUpdateEvent {
            model: None,
            openai_api_key: None,
            use_behavior_trees: Some(behavior_trees),
            openai_tracing_enabled: Some(openai_tracing),
            web_search_enabled: Some(web_search),
            brave_api_key: None,
        });
    refresh_settings_from_db(conn, state)?;
    Ok(())
}

pub fn submit_brave_api_key(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    behavior_trees: bool,
    openai_tracing: bool,
    web_search_enabled: bool,
    api_key: String,
) -> Result<(), String> {
    update_settings_in_db(conn, behavior_trees, openai_tracing, web_search_enabled)?;
    let _ = bus
        .ui_to_agent_tx
        .send(UiToAgentEvent::SettingsUpdateEvent {
            model: None,
            openai_api_key: None,
            use_behavior_trees: Some(behavior_trees),
            openai_tracing_enabled: Some(openai_tracing),
            web_search_enabled: Some(web_search_enabled),
            brave_api_key: Some(api_key),
        });
    refresh_settings_from_db(conn, state)?;
    state.mode = UiMode::Normal;
    Ok(())
}
