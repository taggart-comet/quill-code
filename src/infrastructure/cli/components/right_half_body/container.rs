use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;

use super::{file_changes, todo_list};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    // Determine what to show
    let has_file_changes = state.file_changes.is_some();
    let has_todo_list = state.todo_list.is_some();

    if !has_file_changes && !has_todo_list {
        return;
    }

    // If both exist, split vertically (50/50)
    if has_file_changes && has_todo_list {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        file_changes::render(frame, chunks[0], state, theme);
        todo_list::render(frame, chunks[1], state, theme);
    } else if has_file_changes {
        file_changes::render(frame, area, state, theme);
    } else {
        todo_list::render(frame, area, state, theme);
    }
}
