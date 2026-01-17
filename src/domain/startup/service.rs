use super::Error;
use crate::domain::prompting::session_naming_prompt;
use crate::domain::workflow::Chain;
use crate::domain::{Project, Session, SessionRequest};
use crate::infrastructure::inference::InferenceEngine;
use crate::repository::{ProjectsRepository, SessionRequestsRepository, SessionsRepository};
use rusqlite::Connection;
use std::env;
use std::sync::Arc;

/// Configuration for the startup service
#[derive(Debug, Clone)]
pub struct StartupConfig {
    pub debug: bool,
}

/// Domain service responsible for creating sessions and managing domain logic
/// This service operates on already-initialized infrastructure components
pub struct StartupService {
    config: StartupConfig,
    engine: Arc<dyn InferenceEngine>,
    conn: Arc<Connection>,
}

impl StartupService {
    /// Create a new startup service with the given configuration
    pub fn new(
        config: StartupConfig,
        engine: Arc<dyn InferenceEngine>,
        conn: Arc<Connection>,
    ) -> Self {
        Self {
            config,
            engine,
            conn,
        }
    }

    /// Create a startup service with debug configuration
    pub fn with_debug(
        debug: bool,
        engine: Arc<dyn InferenceEngine>,
        conn: Arc<Connection>,
    ) -> Self {
        Self::new(StartupConfig { debug }, engine, conn)
    }

    /// Create a new session with the given first prompt
    /// This is a pure domain operation that uses infrastructure components
    ///
    /// # Arguments
    /// * `first_prompt` - The initial user prompt
    ///
    /// # Returns
    /// A ready Session entity with the first request and conversation history
    pub fn start(&self, first_prompt: &str) -> Result<Session, Error> {
        // 1. Get or create project based on current directory name
        let project = self.init_project()?;

        // 2. Create session with the first prompt
        self.create_session(&project, first_prompt)
    }

    /// Initialize or load a project based on the current directory
    fn init_project(&self) -> Result<Project, Error> {
        let current_dir = env::current_dir().ok();

        let project_name = current_dir
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".to_string());

        let project_root = current_dir
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        let repo = ProjectsRepository::new(&self.conn);
        let (row, created) = repo
            .get_or_create(&project_name, &project_root)
            .map_err(Error::Repository)?;

        // Map repository row to domain entity
        let project = Project::from(row);

        if created {
            log::info!("Project created: {} (id={})", project.name(), project.id());
        } else {
            log::info!(
                "Project loaded: {} (id={}, sessions={})",
                project.name(),
                project.id(),
                project.session_count()
            );
        }

        Ok(project)
    }

    /// Create a new session with the given first prompt
    fn create_session(&self, project: &Project, first_prompt: &str) -> Result<Session, Error> {
        // Generate session name from the first prompt using chat format
        let prompt_preview: String = first_prompt.chars().take(100).collect();
        let naming_prompt = session_naming_prompt(self.engine.get_type(), &prompt_preview);

        let system_prompt = crate::domain::prompting::get_system_prompt(self.engine.get_type());
        let chain = Chain::new();
        let session_name = match self
            .engine
            .generate(&system_prompt, &naming_prompt, 15, &[], &chain)
        {
            Ok(raw) => {
                log::debug!("Raw session name response: {:?}", raw.summary);
                // Clean up the response
                let cleaned = raw
                    .summary
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('*')
                    .replace("<|im_end|>", "")
                    .trim()
                    .to_string();

                if cleaned.is_empty() || cleaned.len() > 50 {
                    Self::fallback_session_name(first_prompt)
                } else {
                    cleaned
                }
            }
            Err(e) => {
                log::warn!("Session naming failed: {}", e);
                Self::fallback_session_name(first_prompt)
            }
        };

        // Create session in database
        let sessions_repo = SessionsRepository::new(&self.conn);
        let row = sessions_repo
            .create(project.id(), &session_name)
            .map_err(Error::Repository)?;

        // Increment project session count
        let projects_repo = ProjectsRepository::new(&self.conn);
        projects_repo
            .increment_session_count(project.id())
            .map_err(Error::Repository)?;

        // Create session without requests (requests will be created by session_service.run())
        let session = Session::from_row_with_project(row, project.clone());

        log::info!(
            "Session created: \"{}\" (id={})",
            session.name(),
            session.id()
        );

        Ok(session)
    }

    /// Generate a fallback session name from the prompt
    fn fallback_session_name(prompt: &str) -> String {
        // Use first few words of the prompt as fallback
        let words: Vec<&str> = prompt.split_whitespace().take(5).collect();
        let name = words.join(" ");
        if name.len() > 40 {
            format!("{}...", &name[..37])
        } else if name.is_empty() {
            "New Session".to_string()
        } else {
            name
        }
    }

    /// Load an existing session by ID with all its requests
    pub fn load_session(&self, session_id: i64) -> Result<Session, Error> {
        let sessions_repo = SessionsRepository::new(&self.conn);
        let session_row = sessions_repo
            .find_by_id(session_id)
            .map_err(Error::Repository)?
            .ok_or(Error::SessionNotFound(session_id))?;

        // Get project entity
        let projects_repo = ProjectsRepository::new(&self.conn);
        let project_row = projects_repo
            .find_by_id(session_row.project_id)
            .map_err(Error::Repository)?
            .ok_or_else(|| {
                Error::Repository(format!(
                    "Project with id {} not found",
                    session_row.project_id
                ))
            })?;
        let project = Project::from(project_row);

        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let request_rows = requests_repo
            .find_by_session(session_id)
            .map_err(Error::Repository)?;
        let requests: Vec<SessionRequest> =
            request_rows.into_iter().map(SessionRequest::from).collect();

        let session = Session::load_with_requests(session_row, project, requests);
        Ok(session)
    }

    /// Add a new request to an existing session
    pub fn add_request(&self, session: &mut Session, prompt: &str) -> Result<(), Error> {
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let request_row = requests_repo
            .create(session.id(), prompt)
            .map_err(Error::Repository)?;
        let request = SessionRequest::from(request_row);
        session.add_request(request);
        session.set_current_request(prompt.to_string());
        Ok(())
    }

    /// Update the result of the latest request in a session
    pub fn update_latest_result(
        &self,
        session: &mut Session,
        result_summary: &str,
    ) -> Result<(), Error> {
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        if let Some(latest_request) = requests_repo
            .find_latest_by_session(session.id())
            .map_err(Error::Repository)?
        {
            requests_repo
                .update_result(latest_request.id, result_summary)
                .map_err(Error::Repository)?;

            // Reload the session to get the updated request
            *session = self.load_session(session.id())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_session_name() {
        assert_eq!(
            StartupService::fallback_session_name("Hello world test"),
            "Hello world test"
        );

        // Test with a long prompt that will be truncated after taking 5 words
        let result = StartupService::fallback_session_name(
            "This is a very long prompt with many words that should be truncated",
        );
        assert_eq!(result, "This is a very long"); // Only first 5 words, under 40 chars

        // Test with 5 words that exceed 40 characters
        let result2 = StartupService::fallback_session_name("supercalifragilisticexpialidocious antidisestablishmentarianism pneumonoultramicroscopicsilicovolcanoconiosisword anotherverylongword");
        assert!(result2.len() <= 40);
        assert!(result2.ends_with("..."));

        assert_eq!(StartupService::fallback_session_name(""), "New Session");
    }

    #[test]
    fn test_startup_config() {
        let config = StartupConfig { debug: false };
        assert!(!config.debug);

        // Note: This test would need mock engine and conn to work properly
        // For now, we just test the config structure
        assert!(config.debug == false);
    }
}
