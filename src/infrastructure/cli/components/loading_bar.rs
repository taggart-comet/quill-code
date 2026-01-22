use crate::infrastructure::cli::helpers::widgets::{
    loading_bar_line,
    loading_bar_line_with_colors,
};
use ratatui::style::Color;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

pub fn render(frame: &mut Frame, area: Rect, ratio: f64) {
    let bar_width = (area.width / 4).max(12).min(30) as usize;
    let bar_line = loading_bar_line(bar_width, ratio);
    let bar = Paragraph::new(Text::from(bar_line)).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(bar, area);
}

pub fn render_with_colors(
    frame: &mut Frame,
    area: Rect,
    ratio: f64,
    active_color: Color,
    inactive_color: Color,
) {
    let bar_width = (area.width / 4).max(12).min(30) as usize;
    let bar_line = loading_bar_line_with_colors(bar_width, ratio, active_color, inactive_color);
    let bar = Paragraph::new(Text::from(bar_line)).alignment(ratatui::layout::Alignment::Left);
    frame.render_widget(bar, area);
}
