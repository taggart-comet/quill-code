use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::layout::Alignment;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let title = state.header_title.clone().unwrap_or_else(|| {
        "QuillCode, your not so capable coding companion is ready to make some slop!".to_string()
    });
    let header = Paragraph::new(title)
        .alignment(Alignment::Left)
        .style(Style::default().fg(theme.info_text))
        .block(panel_block(theme, theme.header_bg, PANEL_PADDING));
    frame.render_widget(header, area);
}
