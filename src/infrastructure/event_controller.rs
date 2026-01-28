use crate::domain::permissions::{PermissionConfig, PermissionDecision, PermissionPrompter};
use crate::domain::{CancellationToken, SessionService, StartupService};
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{
    AgentToUiEvent, EventBus, PermissionUpdate, RequestStatus, StepPhase, UiToAgentEvent,
};
use crate::infrastructure::inference::InferenceEngine;
use crate::infrastructure::init::{
    apply_model_selection, update_openai_api_key,
};
use crate::repository::UserSettingsRepository;
use crate::{domain, infrastructure};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// Helper function to send a failure event to the UI
fn send_failure_event(
    tx: &Sender<AgentToUiEvent>,
    request_id: u64,
    error: impl Into<String>,
) {
    let _ = tx.send(AgentToUiEvent::RequestFinishedEvent {
        request_id,
        status: RequestStatus::Failure,
        summary: Some(error.into()),
        final_message: None,
    });
}

/// Macro to send a failure event and continue to the next iteration
macro_rules! send_failure_and_continue {
    ($self:expr, $error:expr) => {{
        send_failure_event(&$self.bus.agent_to_ui_tx, 0, $error);
        continue;
    }};
}

/// Helper function to apply multiple settings updates
fn apply_settings_updates(
    conn_guard: &PooledConnection<SqliteConnectionManager>,
    use_behavior_trees: Option<bool>,
    openai_tracing_enabled: Option<bool>,
    web_search_enabled: Option<bool>,
    brave_api_key: Option<&str>,
) -> Result<(), String> {
    let settings_repo = UserSettingsRepository::new(&**conn_guard);

    if let Some(use_behavior_trees) = use_behavior_trees {
        settings_repo.update_use_behavior_trees(use_behavior_trees)?;
    }

    if let Some(openai_tracing_enabled) = openai_tracing_enabled {
        settings_repo.update_openai_tracing_enabled(openai_tracing_enabled)?;
    }

    if let Some(web_search_enabled) = web_search_enabled {
        settings_repo.update_web_search_enabled(web_search_enabled)?;
    }

    if let Some(api_key) = brave_api_key {
        settings_repo.update_brave_api_key(Some(api_key))?;
    }

    Ok(())
}

pub struct EventController {
    bus: EventBus,
    conn: DbPool,
    engine: Option<Arc<dyn InferenceEngine>>,
    app_name: String,
    use_behavior_trees: bool,
    openai_tracing_enabled: bool,
    web_search_enabled: bool,
    current_session_id: Option<i64>,
    cancel_token: CancellationToken,
    permission_response_tx: Option<Sender<PermissionUpdate>>,
    request_counter: u64,
}

impl EventController {
    pub fn new(
        bus: EventBus,
        conn: DbPool,
        engine: Option<Arc<dyn InferenceEngine>>,
        app_name: String,
    ) -> Result<Self, String> {
        let conn_guard = conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;
        let settings_repo = UserSettingsRepository::new(&*conn_guard);
        let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
        drop(conn_guard);
        let use_behavior_trees = settings.use_behavior_trees;
        let openai_tracing_enabled = settings.openai_tracing_enabled;
        let web_search_enabled = settings.web_search_enabled;

        Ok(Self {
            bus,
            conn,
            engine,
            app_name,
            use_behavior_trees,
            openai_tracing_enabled,
            web_search_enabled,
            current_session_id: None,
            cancel_token: CancellationToken::new(),
            permission_response_tx: None,
            request_counter: 1,
        })
    }

    pub fn run(mut self) -> Result<(), String> {
        let (worker_status_tx, worker_status_rx) = unbounded::<()>();
        let mut worker_running = false;

        loop {
            select! {
                recv(worker_status_rx) -> _ => {
                    worker_running = false;
                    self.permission_response_tx = None;
                }
                recv(self.bus.ui_to_agent_rx) -> msg => {
                    let event = match msg {
                        Ok(event) => event,
                        Err(_) => break,
                    };

                    match event {
                        UiToAgentEvent::RequestEvent { prompt, images, mode } => {
                            if worker_running {
                                send_failure_and_continue!(self, "Request already running");
                            }

                            let engine = match self.engine.clone() {
                                Some(engine) => engine,
                                None => {
                                    send_failure_and_continue!(self, "No model selected");
                                }
                            };

                            let session_id = match self.ensure_session(&engine, &prompt) {
                                Ok(id) => id,
                                Err(err) => {
                                    send_failure_and_continue!(self, err);
                                }
                            };

                            self.cancel_token.reset();
                            worker_running = true;

                            let request_id = self.request_counter;
                            self.request_counter = self.request_counter.saturating_add(1);
                            let label = request_label(&prompt);
                            let _ = self.bus.agent_to_ui_tx.send(AgentToUiEvent::RequestStartedEvent {
                                    request_id,
                                    label,
                                    prompt: prompt.clone(),
                                });

                            let (permission_response_tx, permission_response_rx) = unbounded();
                            self.permission_response_tx = Some(permission_response_tx);

                            let cancel_token = self.cancel_token.clone();
                            let app_name = self.app_name.clone();
                            let use_behavior_trees = self.use_behavior_trees;
                            let worker_status_tx = worker_status_tx.clone();

                            // Convert ImageAttachment to data URLs
                            let image_data_urls: Vec<String> = images.iter().map(|img| img.data_url.clone()).collect();

                            let event_bus = Arc::new(self.bus.clone());
                            thread::spawn(move || {
                                let result = run_request_worker(
                                    &app_name,
                                    engine,
                                    request_id,
                                    session_id,
                                    prompt,
                                    image_data_urls,
                                    mode,
                                    use_behavior_trees,
                                    cancel_token,
                                    permission_response_rx,
                                    event_bus.clone(),
                                );

                                if let Err(error) = result {
                                    send_failure_event(&event_bus.agent_to_ui_tx, request_id, error);
                                }

                                let _ = worker_status_tx.send(());
                            });
                        }
                        UiToAgentEvent::PermissionUpdateEvent {
                            request_id,
                            decision,
                        } => {
                            if let Some(sender) = &self.permission_response_tx {
                                let _ = sender.send(PermissionUpdate {
                                    request_id,
                                    decision,
                                });
                            }
                        }
                        UiToAgentEvent::ShutdownEvent => {
                            break;
                        }
                        UiToAgentEvent::SettingsUpdateEvent {
                            model,
                            openai_api_key,
                            use_behavior_trees,
                            openai_tracing_enabled,
                            web_search_enabled,
                            brave_api_key,
                        } => {
                            if let Some(api_key) = openai_api_key.as_deref() {
                                if let Err(err) = update_openai_api_key(&self.conn, api_key) {
                                    send_failure_and_continue!(self, err.to_string());
                                }
                            }

                            if let Some(selection) = model {
                                match apply_model_selection(&self.conn, selection) {
                                    Ok(engine) => {
                                        self.engine = Some(engine);
                                        self.current_session_id = None;
                                    }
                                    Err(err) => {
                                        send_failure_and_continue!(self, err.to_string());
                                    }
                                }
                            }

                            // Lock connection for settings updates
                            let conn_guard = match self.conn.get() {
                                Ok(guard) => guard,
                                Err(e) => {
                                    send_failure_and_continue!(self, format!("Failed to get connection: {}", e));
                                }
                            };

                            // Apply all settings updates
                            if let Err(err) = apply_settings_updates(
                                &conn_guard,
                                use_behavior_trees,
                                openai_tracing_enabled,
                                web_search_enabled,
                                brave_api_key.as_deref(),
                            ) {
                                send_failure_and_continue!(self, err);
                            }

                            // Reload settings from database
                            if let Err(err) = self.reload_settings_from_db(&conn_guard) {
                                send_failure_and_continue!(self, err);
                            }

                            let _ = self.bus.agent_to_ui_tx.send(self.settings_snapshot());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn ensure_session(
        &mut self,
        engine: &Arc<dyn InferenceEngine>,
        first_prompt: &str,
    ) -> Result<i64, String> {
        if let Some(id) = self.current_session_id {
            return Ok(id);
        }

        let startup_service = StartupService::new(engine.clone(), self.conn.clone());
        let session = startup_service
            .start(first_prompt)
            .map_err(|e| e.to_string())?;
        let _ = self
            .bus
            .agent_to_ui_tx
            .send(AgentToUiEvent::SessionStartedEvent {
                title: session.name().to_string(),
            });
        self.current_session_id = Some(session.id());
        Ok(session.id())
    }

    fn settings_snapshot(&self) -> AgentToUiEvent {
        AgentToUiEvent::SettingsSnapshot
    }

    /// Reload settings from database and update internal state
    fn reload_settings_from_db(
        &mut self,
        conn_guard: &PooledConnection<SqliteConnectionManager>,
    ) -> Result<(), String> {
        let settings_repo = UserSettingsRepository::new(&**conn_guard);
        let settings = settings_repo.get_current()?;
        self.use_behavior_trees = settings.use_behavior_trees;
        self.openai_tracing_enabled = settings.openai_tracing_enabled;
        self.web_search_enabled = settings.web_search_enabled;
        Ok(())
    }
}

fn run_request_worker(
    app_name: &str,
    engine: Arc<dyn InferenceEngine>,
    request_id: u64,
    session_id: i64,
    prompt: String,
    images: Vec<String>,
    mode: crate::domain::AgentModeType, // NEW: Agent mode
    use_behavior_trees: bool,
    cancel_token: CancellationToken,
    permission_response_rx: Receiver<PermissionUpdate>,
    event_bus: Arc<EventBus>,
) -> Result<(), String> {
    let worker_conn = infrastructure::db::init_db(app_name)
        .map_err(|e| format!("Failed to open worker database: {}", e))?;

    let prompter = Arc::new(BusPermissionPrompter::new(
        event_bus.agent_to_ui_tx.clone(),
        permission_response_rx,
    ));

    let mut session_service = SessionService::new(
        engine.clone(),
        worker_conn.clone(),
        use_behavior_trees,
        PermissionConfig::default(),
        prompter,
        event_bus.agent_to_ui_tx.clone()
    )
    .map_err(|e| format!("Failed to create session service: {}", e))?;

    let startup_service = StartupService::new(engine.clone(), worker_conn.clone());
    let session = startup_service
        .load_session(session_id)
        .map_err(|e| e.to_string())?;

    let result = session_service.run(&session, &prompt, &images, mode, &cancel_token);
    match result {
        Ok(chain) => {
            emit_chain_progress(event_bus.agent_to_ui_tx.clone(), &chain);
            if chain.is_failed {
                let reason = if chain.fail_reason.is_empty() {
                    "Workflow failed".to_string()
                } else {
                    chain.fail_reason
                };
                let _ = event_bus.agent_to_ui_tx.send(AgentToUiEvent::RequestFinishedEvent {
                    request_id,
                    status: RequestStatus::Failure,
                    summary: Some(reason),
                    final_message: None,
                });
            } else {
                let _ = event_bus.agent_to_ui_tx.send(AgentToUiEvent::RequestFinishedEvent {
                    request_id,
                    status: RequestStatus::Success,
                    summary: Some(chain.get_summary()),
                    final_message: chain.final_message.map(|msg| msg.to_string()),
                });
            }
            Ok(())
        }
        Err(domain::session::ServiceError::Workflow(domain::workflow::Error::Cancelled)) => {
            let _ = event_bus.agent_to_ui_tx.send(AgentToUiEvent::RequestFinishedEvent {
                request_id,
                status: RequestStatus::Cancelled,
                summary: Some("Cancelled".to_string()),
                final_message: None,
            });
            Ok(())
        }
        Err(err) => Err(format!("Workflow error: {}", err)),
    }
}

fn request_label(prompt: &str) -> String {
    let trimmed = prompt.lines().next().unwrap_or("").trim();
    let mut label = if trimmed.is_empty() {
        "Request".to_string()
    } else {
        trimmed.to_string()
    };
    let max_len = 48;
    if label.len() > max_len {
        label.truncate(max_len.saturating_sub(3));
        label.push_str("...");
    }
    label
}

fn emit_chain_progress(
    agent_tx: crossbeam_channel::Sender<AgentToUiEvent>,
    chain: &domain::workflow::Chain,
) {
    for step in chain.steps() {
        let step_name = step
            .tool_name
            .clone()
            .unwrap_or_else(|| step.step_type.clone());
        let summary = step.summary.clone();

        let _ = agent_tx.send(AgentToUiEvent::ProgressEvent {
            step_name: step_name.clone(),
            phase: StepPhase::Before,
            summary: summary.clone(),
        });
        let _ = agent_tx.send(AgentToUiEvent::ProgressEvent {
            step_name,
            phase: StepPhase::After,
            summary,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::permissions::{PermissionDecision, PermissionRequest, PermissionScope};
    use crate::infrastructure::db;
    use crate::infrastructure::event_bus::{ModelSelection, UiToAgentEvent};
    use crate::repository::{MetaRepository, ModelsRepository, UserSettingsRepository};
    use crossbeam_channel::unbounded;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_app_name() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("drastis-test-{}", nanos)
    }

    #[test]
    fn settings_update_persists_user_settings() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let bus = EventBus::new();
        let controller = EventController::new(bus.clone(), conn.clone(), None, app_name.clone())
            .expect("controller");

        let events = Arc::new(Mutex::new(Vec::new()));
        let receiver_events = Arc::clone(&events);
        let agent_rx = bus.agent_to_ui_rx.clone();
        let receiver = thread::spawn(move || {
            while let Ok(event) = agent_rx.recv() {
                receiver_events.lock().expect("lock events").push(event);
            }
        });

        let sender = bus.ui_to_agent_tx.clone();
        thread::spawn(move || {
            let _ = sender.send(UiToAgentEvent::SettingsUpdateEvent {
                model: None,
                openai_api_key: None,
                use_behavior_trees: Some(true),
                openai_tracing_enabled: None,
                web_search_enabled: None,
                brave_api_key: None,
            });
            let _ = sender.send(UiToAgentEvent::ShutdownEvent);
        });

        controller.run().expect("controller run");
        drop(bus);
        let _ = receiver.join();

        let conn_guard = conn.get().expect("db conn");
        let settings_repo = UserSettingsRepository::new(&*conn_guard);
        let stored = settings_repo.get_current().expect("get settings");
        assert!(stored.use_behavior_trees);

        let snapshot = events
            .lock()
            .expect("lock events")
            .iter()
            .find_map(|event| match event {
                AgentToUiEvent::SettingsSnapshot => Some(true),
                _ => None,
            });
        assert_eq!(snapshot, Some(true));
    }

    #[test]
    fn model_update_creates_openai_model_entry() {
        let app_name = unique_app_name();
        let conn = db::init_db(&app_name).expect("init db");
        let bus = EventBus::new();
        let controller = EventController::new(bus.clone(), conn.clone(), None, app_name.clone())
            .expect("controller");

        let agent_rx = bus.agent_to_ui_rx.clone();
        let receiver = thread::spawn(move || while let Ok(_event) = agent_rx.recv() {});

        let sender = bus.ui_to_agent_tx.clone();
        thread::spawn(move || {
            let _ = sender.send(UiToAgentEvent::SettingsUpdateEvent {
                model: Some(ModelSelection::OpenAiModel("gpt-test".to_string())),
                openai_api_key: Some("test-key".to_string()),
                use_behavior_trees: None,
                openai_tracing_enabled: None,
                web_search_enabled: None,
                brave_api_key: None,
            });
            let _ = sender.send(UiToAgentEvent::ShutdownEvent);
        });

        controller.run().expect("controller run");
        drop(bus);
        let _ = receiver.join();

        let conn_guard = conn.get().expect("db conn");
        let meta_repo = MetaRepository::new(&*conn_guard);
        let model_id = meta_repo.get_last_used_model_id().expect("model id");
        assert!(model_id.is_some());

        let models_repo = ModelsRepository::new(&*conn_guard);
        let models = models_repo
            .find_by_type(crate::domain::ModelType::OpenAI)
            .expect("openai models");
        let model = models
            .iter()
            .find(|model| model.model_name.as_deref() == Some("gpt-test"));
        assert!(model.is_some());
        let model = model.unwrap();
        assert!(model._api_key.is_none());

        let settings_repo = UserSettingsRepository::new(&*conn_guard);
        let settings = settings_repo.get_current().expect("settings");
        assert_eq!(settings.openai_api_key.as_deref(), Some("test-key"));
    }

    #[test]
    fn permission_prompter_waits_for_update() {
        let (agent_tx, agent_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();
        let prompter = BusPermissionPrompter::new(agent_tx, update_rx);
        let request = PermissionRequest::new(
            "shell_exec".to_string(),
            Some("rm -rf /".to_string()),
            vec![],
            PermissionScope::Project,
            Some(1),
        );

        let handler = thread::spawn(move || {
            prompter
                .ask_permission(&request)
                .expect("permission decision")
        });

        let event = agent_rx.recv().expect("permission event");
        let request_id = match event {
            AgentToUiEvent::PermissionRequestEvent { request_id, .. } => request_id,
            _ => panic!("unexpected event"),
        };

        let _ = update_tx.send(PermissionUpdate {
            request_id,
            decision: PermissionDecision::AlwaysAllow,
        });

        let decision = handler.join().expect("join handler");
        assert_eq!(decision, PermissionDecision::AlwaysAllow);
    }
}

struct BusPermissionPrompter {
    agent_tx: Sender<AgentToUiEvent>,
    response_rx: Receiver<PermissionUpdate>,
    counter: Arc<AtomicU64>,
}

impl BusPermissionPrompter {
    fn new(agent_tx: Sender<AgentToUiEvent>, response_rx: Receiver<PermissionUpdate>) -> Self {
        Self {
            agent_tx,
            response_rx,
            counter: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl PermissionPrompter for BusPermissionPrompter {
    fn ask_permission(
        &self,
        request: &domain::permissions::PermissionRequest,
    ) -> Result<PermissionDecision, crate::utils::AskError> {
        let request_id = self.counter.fetch_add(1, Ordering::SeqCst);
        let scope = match request.scope {
            domain::permissions::PermissionScope::Session => "session",
            domain::permissions::PermissionScope::Project => "project",
            domain::permissions::PermissionScope::Global => "global",
        };
        let paths = request
            .paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        let _ = self.agent_tx.send(AgentToUiEvent::PermissionRequestEvent {
            request_id,
            tool_name: request.tool_name.clone(),
            command: request.command.clone(),
            paths,
            scope: scope.to_string(),
        });

        loop {
            match self.response_rx.recv() {
                Ok(update) if update.request_id == request_id => return Ok(update.decision),
                Ok(_) => continue,
                Err(_) => {
                    return Err(crate::utils::AskError::IoError);
                }
            }
        }
    }
}
