use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let info = format!("Model: {} | Press \"/\" for Menu", state.current_model);
    let panel = Paragraph::new(info)
        .style(Style::default().fg(theme.info_text))
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}
