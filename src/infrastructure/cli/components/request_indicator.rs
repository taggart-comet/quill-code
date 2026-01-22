use crate::infrastructure::cli::components::loading_bar;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    if area.height == 0 {
        return;
    }

    let ratio = if state.request_status.is_some() {
        1.0
    } else if state.request_in_flight.is_some() {
        state.loading_bar.ratio()
    } else {
        return;
    };
    let active_color = theme.success;

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let target = sections[0];
    loading_bar::render_with_colors(frame, target, ratio, active_color, inactive_color(theme));
}

fn inactive_color(theme: Theme) -> Color {
    theme.border
}
