use crate::infrastructure::auth::initiate_oauth_flow;
use crate::infrastructure::cli::repl::refresh_settings_from_db;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{EventBus, UiToAgentEvent};
use crate::repository::UserSettingsRepository;

pub fn start_oauth_flow(
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
) -> Result<(), String> {
    log::info!("Starting OpenAI OAuth login flow...");

    // Initiate OAuth flow (blocks until complete or timeout)
    let tokens = initiate_oauth_flow()?;

    // Store tokens in database
    let conn_guard = conn
        .get()
        .map_err(|e| format!("Database connection failed: {}", e))?;
    let settings_repo = UserSettingsRepository::new(&*conn_guard);

    settings_repo.update_oauth_tokens(
        &tokens.access_token,
        &tokens.refresh_token,
        tokens.expires_in,
        tokens.account_id.as_deref(),
    )?;

    log::info!("OAuth tokens stored successfully");

    // Notify event controller to reload engine with OAuth credentials
    let _ = bus.ui_to_agent_tx.send(UiToAgentEvent::SettingsUpdateEvent {
        model: None,
        openai_api_key: None,
        use_behavior_trees: None,
        openai_tracing_enabled: None,
        web_search_enabled: None,
        max_tool_calls_per_request: None,
        brave_api_key: None,
    });

    // Refresh UI state from database
    refresh_settings_from_db(conn, state)?;

    Ok(())
}
