use super::{request::Request, session_request::SessionRequest};
use crate::domain::AgentModeType;
use crate::domain::{Project, UserSettings};
use crate::infrastructure::db::DbPool;
use crate::repository::SessionRow;
use std::path::Path;

/// Domain entity representing a conversation session.
/// A session belongs to a project and contains a named conversation history.
#[derive(Debug, Clone)]
pub struct Session {
    id: i64,
    project_id: i64,
    project: Project,
    name: String,
    _created_at: u64,
    history_from_request_id: Option<i64>,
    requests: Vec<SessionRequest>,
    current_request: String,
    current_user_settings: Option<UserSettings>,
    final_message: Option<String>,
    current_images: Vec<String>,
    current_mode: AgentModeType,
    conn: Option<DbPool>,
}

impl Session {
    pub fn id(&self) -> i64 {
        self.id
    }

    #[allow(dead_code)]
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_current_request(&mut self, prompt: String) {
        self.current_request = prompt;
    }

    pub fn set_current_images(&mut self, images: Vec<String>) {
        self.current_images = images;
    }

    pub fn set_current_mode(&mut self, mode: AgentModeType) {
        self.current_mode = mode;
    }

    pub fn set_current_user_settings(&mut self, settings: Option<UserSettings>) {
        self.current_user_settings = settings;
    }

    pub fn set_requests(&mut self, requests: Vec<SessionRequest>) {
        self.requests = requests;
    }

    pub fn set_conn(&mut self, conn: DbPool) {
        self.conn = Some(conn);
    }

    pub fn load_with_requests(
        session_row: crate::repository::SessionRow,
        project: Project,
        requests: Vec<SessionRequest>,
    ) -> Self {
        let mut session = Self::from_row_with_project(session_row, project);
        session.set_requests(requests);
        session
    }

    /// Create a Session from a SessionRow with a Project entity
    pub fn from_row_with_project(row: SessionRow, project: Project) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            project,
            name: row.name,
            _created_at: row.created_at.parse().unwrap_or(0),
            history_from_request_id: row.history_from_request_id,
            requests: Vec::new(),
            current_request: String::new(),
            current_user_settings: None,
            final_message: None,
            current_images: Vec::new(),
            current_mode: AgentModeType::Build,
            conn: None,
        }
    }
}

impl Request for Session {
    fn history(&self) -> &[SessionRequest] {
        &self.requests
    }

    fn current_request(&self) -> &str {
        &self.current_request
    }

    fn mode(&self) -> AgentModeType {
        self.current_mode
    }

    fn project_root(&self) -> &Path {
        self.project.project_root()
    }

    fn user_settings(&self) -> Option<&UserSettings> {
        self.current_user_settings.as_ref()
    }

    fn project_id(&self) -> Option<i32> {
        Some(self.project_id as i32)
    }

    fn set_final_message(&mut self, message: String) {
        self.final_message = Some(message);
    }

    fn images(&self) -> &[String] {
        &self.current_images
    }

    fn session_id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn get_history_steps(&self) -> Vec<crate::domain::workflow::step::ChainStep> {
        use crate::repository::SessionRequestStepsRepository;

        // BuildFromPlan keeps context minimal — TODO list (with statuses) is
        // already included as a system message by the chain's todo_list field.
        if self.current_mode == AgentModeType::BuildFromPlan {
            return Vec::new();
        }

        // Return empty if no connection available
        let conn = match &self.conn {
            Some(c) => c.clone(),
            None => {
                log::warn!("No database connection available for loading history steps");
                return Vec::new();
            }
        };

        let steps_repo = SessionRequestStepsRepository::new(conn.clone());

        // Load all steps from all requests in this session
        let steps = match steps_repo.load_steps_for_session(self.id, self.history_from_request_id) {
            Ok(steps) => steps,
            Err(e) => {
                log::warn!(
                    "Failed to load history steps for session {}: {}",
                    self.id,
                    e
                );
                Vec::new()
            }
        };

        steps
    }

    fn get_session_plan(&self) -> Option<crate::domain::todo::TodoList> {
        use crate::repository::TodoListRepository;

        // Return None if no connection available
        let conn = match &self.conn {
            Some(c) => c.clone(),
            None => {
                log::warn!("No database connection available for loading TODO list");
                return None;
            }
        };

        let conn_guard = match conn.get() {
            Ok(guard) => guard,
            Err(e) => {
                log::warn!("Failed to get database connection: {}", e);
                return None;
            }
        };

        let repo = TodoListRepository::new(&*conn_guard);

        // Get TODO list for this session
        match repo.get_by_session(self.id) {
            Ok(Some(row)) => {
                // Parse the JSON content into TodoList
                match serde_json::from_str::<crate::domain::todo::TodoList>(&row.content) {
                    Ok(todo_list) => Some(todo_list),
                    Err(e) => {
                        log::warn!("Failed to parse TODO list JSON: {}", e);
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("Failed to load TODO list for session {}: {}", self.id, e);
                None
            }
        }
    }
}

// Note: From<SessionRow> is no longer implemented because Session requires a Project entity.
// Use Session::from_row_with_project() instead.