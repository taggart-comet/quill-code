use crate::domain::permissions::UserPermissionDecision;
use crate::domain::session::ServiceError;
use crate::domain::todo::{TodoList, TodoListStatus};
use crate::domain::workflow::{CancellationToken, Chain, Error as WorkflowError};
use crate::domain::SessionService;
use crate::domain::{AgentModeType, Project, Session, UserSettings};
use crate::infrastructure::event_bus::{AgentToUiEvent, PermissionUpdate, StepPhase};
use crate::repository::SessionRequestsRepository;
use crate::repository::{ProjectsRepository, SessionsRepository, TodoListRepository};
use crossbeam_channel::Receiver;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PlanItem {
    pub index: usize,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub items: Vec<PlanItem>,
}

pub struct PlanService {
    confirmation_rx: Option<Receiver<PermissionUpdate>>,
    event_sender: crossbeam_channel::Sender<AgentToUiEvent>,
    conn: crate::infrastructure::db::DbPool,
}

impl PlanService {
    pub fn new(
        conn: crate::infrastructure::db::DbPool,
        event_sender: crossbeam_channel::Sender<AgentToUiEvent>,
        confirmation_rx: Option<Receiver<PermissionUpdate>>,
    ) -> Self {
        Self {
            confirmation_rx,
            event_sender,
            conn,
        }
    }

    pub fn from_todo_list(_session_id: i64, todo_list: TodoList) -> Option<Plan> {
        if todo_list.items.is_empty() || todo_list.is_completed() {
            return None;
        }

        let items = todo_list
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.status != TodoListStatus::Completed)
            .map(|(index, item)| PlanItem {
                index,
                title: item.title.clone(),
                description: item.description.clone(),
            })
            .collect();

        Some(Plan { items })
    }

    pub fn execute(
        &self,
        service: &mut SessionService,
        plan: Plan,
        session: &Session,
        images: &[String],
        user_settings: &UserSettings,
        cancel: &CancellationToken,
    ) -> Result<Chain, ServiceError> {
        let mut last_chain: Option<Chain> = None;

        for (position, item) in plan.items.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(ServiceError::Workflow(WorkflowError::Cancelled));
            }

            self.emit_progress(&item.title, StepPhase::Before);
            self.update_todo_status(session.id(), item.index, TodoListStatus::InProgress);

            let sub_prompt = format!(
                "## Task: {}\n\n{}\n\nComplete this task using available tools. When done, provide a summary of what was accomplished.",
                item.title, item.description
            );

            let sub_session = self.create_sub_session(session.project_id(), item.index)?;

            let chain_result = service.build(
                &sub_session,
                &sub_prompt,
                images,
                user_settings,
                AgentModeType::Build,
                cancel,
            );

            match chain_result {
                Ok(chain) => {
                    self.save_sub_agent_result_as_session_request(
                        session.id(),
                        &sub_prompt,
                        &chain,
                    )?;

                    self.update_todo_status(session.id(), item.index, TodoListStatus::Completed);
                    self.emit_progress(&item.title, StepPhase::After);
                    last_chain = Some(chain);

                    if !self.confirm_next_item(&plan.items, position, cancel, &item.title)? {
                        let mut chain_to_return = last_chain.unwrap_or_else(Chain::new);
                        chain_to_return.set_final_message(
                            "Stopped by user after completing: ".to_string() + &item.title,
                        );
                        return Ok(chain_to_return);
                    }
                }
                Err(e) => {
                    self.emit_failed_progress(&item.title);
                    return Err(e);
                }
            }
        }

        Ok(last_chain.unwrap_or_else(|| {
            let mut chain = Chain::new();
            chain.set_final_message("All TODO items completed.".to_string());
            chain
        }))
    }

    fn create_sub_session(&self, project_id: i64, index: usize) -> Result<Session, ServiceError> {
        let conn = self.conn.get().map_err(|e| {
            ServiceError::Repository(format!("Failed to get database connection: {}", e))
        })?;
        let sessions_repo = SessionsRepository::new(&*conn);
        let projects_repo = ProjectsRepository::new(&*conn);
        let project_row = projects_repo
            .find_by_id(project_id)
            .map_err(ServiceError::Repository)?
            .ok_or_else(|| {
                ServiceError::Repository(format!("Project {} not found for session", project_id))
            })?;
        let session_row = sessions_repo
            .create(project_id, &format!("todo-sub-agent-{}", index + 1))
            .map_err(ServiceError::Repository)?;
        Ok(Session::from_row_with_project(
            session_row,
            Project::from(project_row),
        ))
    }

    fn update_todo_status(&self, session_id: i64, index: usize, status: TodoListStatus) {
        if let Err(e) = self.mark_todo_item_status(session_id, index, status) {
            log::error!("Failed to update TODO item {}: {}", index, e);
        }

        if let Some(updated_list) = self.load_todo_list(session_id) {
            let _ = self.event_sender.send(AgentToUiEvent::TodoListUpdateEvent {
                items: updated_list.items,
            });
        }
    }

    fn emit_progress(&self, title: &str, phase: StepPhase) {
        let label = match phase {
            StepPhase::Before => "Starting",
            StepPhase::After => "Completed",
        };

        let _ = self.event_sender.send(AgentToUiEvent::ProgressEvent {
            step_name: "build_from_plan".to_string(),
            phase,
            summary: format!("{}: {}", label, title),
        });
    }

    fn emit_failed_progress(&self, title: &str) {
        let _ = self.event_sender.send(AgentToUiEvent::ProgressEvent {
            step_name: "build_from_plan".to_string(),
            phase: StepPhase::After,
            summary: format!("Failed: {}", title),
        });
    }

    fn confirm_next_item(
        &self,
        items: &[PlanItem],
        position: usize,
        cancel: &CancellationToken,
        title: &str,
    ) -> Result<bool, ServiceError> {
        let confirmation_rx = match &self.confirmation_rx {
            Some(rx) => rx,
            None => return Ok(true),
        };

        let remaining = items.len().saturating_sub(position + 1);
        if remaining == 0 {
            return Ok(true);
        }

        let next_title = &items[position + 1].title;
        let total = items.len();
        let completed_count = total - remaining;
        let request_id = 1_000_000 + completed_count as u64;

        let _ = self
            .event_sender
            .send(AgentToUiEvent::PermissionRequestEvent {
                request_id,
                tool_name: "build_from_plan".to_string(),
                command: Some(format!("Continue to next: {}", next_title)),
                paths: vec![format!(
                    "Completed {}/{}: {}",
                    completed_count, total, title
                )],
                scope: "session".to_string(),
                is_read_only: false,
            });

        loop {
            if cancel.is_cancelled() {
                return Err(ServiceError::Workflow(WorkflowError::Cancelled));
            }

            match confirmation_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(update) if update.request_id == request_id => match update.decision {
                    UserPermissionDecision::AllowOnce | UserPermissionDecision::AlwaysAllow => {
                        return Ok(true)
                    }
                    UserPermissionDecision::Deny => return Ok(false),
                },
                Ok(_) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    return Err(ServiceError::Workflow(WorkflowError::Cancelled));
                }
            }
        }
    }

    fn load_todo_list(&self, session_id: i64) -> Option<TodoList> {
        let conn = self.conn.get().ok()?;
        let repo = TodoListRepository::new(&*conn);
        let row = repo.get_by_session(session_id).ok()??;
        serde_json::from_str::<TodoList>(&row.content).ok()
    }

    fn mark_todo_item_status(
        &self,
        session_id: i64,
        item_index: usize,
        status: TodoListStatus,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .get()
            .map_err(|e| format!("Failed to get database connection: {}", e))?;
        let repo = TodoListRepository::new(&*conn);

        let row = repo
            .get_by_session(session_id)
            .map_err(|e| format!("Failed to get TODO list: {}", e))?
            .ok_or_else(|| "No TODO list found for session".to_string())?;

        let mut todo_list: TodoList = serde_json::from_str(&row.content)
            .map_err(|e| format!("Failed to parse TODO list: {}", e))?;

        if item_index < todo_list.items.len() {
            todo_list.items[item_index].status = status;
        } else {
            return Err(format!(
                "Item index {} out of bounds (list has {} items)",
                item_index,
                todo_list.items.len()
            ));
        }

        let updated_json = serde_json::to_string(&todo_list)
            .map_err(|e| format!("Failed to serialize TODO list: {}", e))?;

        let todo_list_row = repo
            .get_or_create_for_session(session_id)
            .map_err(|e| format!("Failed to get TODO list row: {}", e))?;

        repo.update_content(todo_list_row.id, &updated_json)
            .map_err(|e| format!("Failed to update TODO list: {}", e))?;

        Ok(())
    }

    fn save_sub_agent_result_as_session_request(
        &self,
        session_id: i64,
        prompt: &str,
        chain: &Chain,
    ) -> Result<i64, ServiceError> {
        let requests_repo = SessionRequestsRepository::new(self.conn.clone());
        let summary = chain.get_summary();

        let merged_changes = chain.merged_file_changes();

        let changes_json = if merged_changes.is_empty() {
            None
        } else {
            Some(
                serde_json::json!({
                    "changes": merged_changes.clone()
                })
                .to_string(),
            )
        };

        let request_row = requests_repo
            .create_with_result_and_changes(
                session_id,
                prompt,
                AgentModeType::Build,
                &summary,
                changes_json.as_deref(),
            )
            .map_err(ServiceError::Repository)?;
        let request_id = request_row.id;

        if !merged_changes.is_empty() {
            let _ = self.event_sender.send(AgentToUiEvent::FileChangesEvent {
                request_id,
                changes: merged_changes,
            });
        }

        Ok(request_id)
    }
}
