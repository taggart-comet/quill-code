use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::widgets::Clear;
use ratatui::{Frame, layout::Rect};

pub fn render(frame: &mut Frame, area: Rect, theme: Theme) -> Rect {
    frame.render_widget(Clear, area);
    let block = panel_block(theme, theme.panel, PANEL_PADDING);
    frame.render_widget(block.clone(), area);
    block.inner(area)
}
