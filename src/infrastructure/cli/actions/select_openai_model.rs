use crate::infrastructure::cli::actions::insert_openai_key;
use crate::infrastructure::cli::repl::{
    refresh_models_from_db, refresh_settings_from_db, update_model_selection_in_db,
};
use crate::infrastructure::cli::state::{LoadStatus, PopupState, UiMode, UiState};
use crate::infrastructure::cli::views::main_view::ModelEntry;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{EventBus, ModelSelection, UiToAgentEvent};

pub fn open_model_popup(conn: &DbPool, state: &mut UiState) {
    let _ = refresh_models_from_db(conn, state);
    state.mode = UiMode::Popup(PopupState::ModelSelect { selected: 0 });
}

pub fn handle_model_entry(
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
    entry: &ModelEntry,
) -> Result<(), String> {
    match entry {
        ModelEntry::Local { path, .. } => {
            update_model_selection_in_db(conn, &ModelSelection::LocalPath(path.clone()))?;
            let _ = bus
                .ui_to_agent_tx
                .send(UiToAgentEvent::SettingsUpdateEvent {
                    model: Some(ModelSelection::LocalPath(path.clone())),
                    openai_api_key: None,
                    use_behavior_trees: None,
                    openai_tracing_enabled: None,
                    web_search_enabled: None,
                    brave_api_key: None,
                });
            refresh_settings_from_db(conn, state)?;
            state.mode = UiMode::Normal;
        }
        ModelEntry::OpenAi { name } => {
            update_model_selection_in_db(conn, &ModelSelection::OpenAiModel(name.clone()))?;
            let _ = bus
                .ui_to_agent_tx
                .send(UiToAgentEvent::SettingsUpdateEvent {
                    model: Some(ModelSelection::OpenAiModel(name.clone())),
                    openai_api_key: None,
                    use_behavior_trees: None,
                    openai_tracing_enabled: None,
                    web_search_enabled: None,
                    brave_api_key: None,
                });
            refresh_settings_from_db(conn, state)?;
            state.mode = UiMode::Normal;
        }
        ModelEntry::OpenAiSelect => {
            if insert_openai_key::api_key_missing(conn)? {
                insert_openai_key::begin_prompt(state, false);
            } else {
                state.models.openai_available_status = LoadStatus::Loading;
                state.openai_fetch_pending = true;
                state.mode = UiMode::Popup(PopupState::OpenAiAvailable {
                    selected: 0,
                    filter: String::new(),
                    cursor: 0,
                });
            }
        }
        ModelEntry::Loading | ModelEntry::Empty => {}
    }

    Ok(())
}

pub fn handle_openai_available_selection(
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
    name: &str,
) -> Result<(), String> {
    update_model_selection_in_db(conn, &ModelSelection::OpenAiModel(name.to_string()))?;
    let _ = bus
        .ui_to_agent_tx
        .send(UiToAgentEvent::SettingsUpdateEvent {
            model: Some(ModelSelection::OpenAiModel(name.to_string())),
            openai_api_key: None,
            use_behavior_trees: None,
            openai_tracing_enabled: None,
            web_search_enabled: None,
            brave_api_key: None,
        });
    refresh_settings_from_db(conn, state)?;
    state.mode = UiMode::Normal;
    Ok(())
}
