use crate::domain::session::Request;
use crate::domain::todo::TodoList;
use crate::domain::tools::{Error, Tool, ToolResult};
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::AgentToUiEvent;
use crate::repository::TodoListRepository;
use crossbeam_channel::Sender;
use serde_json::{json, Value};
use std::sync::Mutex;

pub struct UpdateTodoList {
    input: Mutex<Option<UpdateTodoListInput>>,
    session_id: i64,
    conn: DbPool,
    event_sender: Sender<AgentToUiEvent>,
}

#[derive(Debug, Clone)]
struct UpdateTodoListInput {
    raw: String,
    call_id: String,
}

impl UpdateTodoList {
    pub fn new(session_id: i64, conn: DbPool, event_sender: Sender<AgentToUiEvent>) -> Self {
        Self {
            input: Mutex::new(None),
            session_id,
            conn,
            event_sender,
        }
    }
}

impl Tool for UpdateTodoList {
    fn name(&self) -> &'static str {
        "update_todo_list"
    }

    fn parse_input(&self, input: String, call_id: String) -> Option<Error> {
        let parsed: Result<TodoList, _> = serde_json::from_str(&input);
        match parsed {
            Ok(_) => {
                let mut lock = self.input.lock().unwrap();
                *lock = Some(UpdateTodoListInput {
                    raw: input,
                    call_id,
                });
                None
            }
            Err(e) => Some(Error::Parse(format!("Failed to parse input: {}", e))),
        }
    }

    fn work(&self, _request: &dyn Request) -> ToolResult {
        let input_lock = self.input.lock().unwrap();
        let input = match input_lock.as_ref() {
            Some(inp) => inp.clone(),
            None => {
                return ToolResult::error(
                    self.name().to_string(),
                    String::new(),
                    "No input provided".to_string(),
                    String::new(),
                )
            }
        };
        drop(input_lock);

        // Lock the connection for database access
        let conn = match self.conn.get() {
            Ok(guard) => guard,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    format!("Failed to get database connection: {}", e),
                    input.call_id.clone(),
                )
            }
        };

        // Get or create TODO list
        let repo = TodoListRepository::new(&*conn);
        let todo_list_row = match repo.get_or_create_for_session(self.session_id) {
            Ok(list) => list,
            Err(e) => {
                return ToolResult::error(
                    self.name().to_string(),
                    input.raw.clone(),
                    format!("Failed to get/create TODO list: {}", e),
                    input.call_id.clone(),
                )
            }
        };

        if let Err(e) = repo.update_content(todo_list_row.id, &input.raw) {
            return ToolResult::error(
                self.name().to_string(),
                input.raw.clone(),
                format!("Failed to update TODO list: {}", e),
                input.call_id.clone(),
            );
        }

        // Parse input to extract items and send event
        if let Ok(parsed) = serde_json::from_str::<TodoList>(&input.raw) {
            let _ = self.event_sender.send(AgentToUiEvent::TodoListUpdateEvent {
                items: parsed.items,
            });
        }

        ToolResult::ok(
            self.name().to_string(),
            input.raw,
            "TODO list updated successfully.".to_string(),
            input.call_id,
        )
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": {
                    "type": "array",
                    "description": "List of TODO items to set. This replaces the entire TODO list.",
                    "items": {
                        "type": "object",
                        "required": ["title", "description", "status"],
                        "properties": {
                            "title": {
                                "type": "string",
                                "description": "Short title for the TODO item"
                            },
                            "description": {
                                "type": "string",
                                "description": "Detailed description of what needs to be done, you can use markdown syntax here."
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status of the item"
                            }
                        }
                    }
                }
            }
        })
    }

    fn desc(&self) -> String {
        "Use this in Plan mode to create or update TODO items after exploring the codebase. For every use, provide full TODO list, as it will replace the existing list.\n\
If you're not in Plan mode, only update statuses of TODO items as you finish them.
"
            .to_string()
    }

    fn get_input(&self) -> String {
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .map(|inp| inp.raw.clone())
            .unwrap_or_default()
    }

    fn get_progress_message(&self, _request: &dyn Request) -> String {
        "Updating TODO list...".to_string()
    }

    fn skip_permission_check(&self) -> bool {
        true
    }
}
