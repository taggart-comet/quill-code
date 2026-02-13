use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoListStatus {
    Pending,
    InProgress,
    Completed,
}

impl TodoListStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TodoListStatus::Pending => "pending",
            TodoListStatus::InProgress => "in_progress",
            TodoListStatus::Completed => "completed",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(value: &str) -> Self {
        match value {
            "pending" | "PENDING" | "planned" | "PLANNED" | "todo" | "TODO" => {
                TodoListStatus::Pending
            }
            "in_progress" | "IN_PROGRESS" | "doing" | "DOING" | "started" | "STARTED" => {
                TodoListStatus::InProgress
            }
            "completed" | "COMPLETED" | "done" | "DONE" | "finished" | "FINISHED" => {
                TodoListStatus::Completed
            }
            &_ => TodoListStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub title: String,
    pub description: String,
    pub status: TodoListStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TodoList {
    pub items: Vec<TodoItem>,
}

impl TodoList {
    pub fn is_completed(&self) -> bool {
        self.items
            .iter()
            .all(|item| item.status == TodoListStatus::Completed)
    }
}
