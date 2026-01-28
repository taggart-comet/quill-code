use crate::infrastructure::cli::helpers::widgets::loading_bar_line;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, ratio: f64) {
    let bar_width = (area.width / 4).max(12).min(30) as usize;
    let bar_line = loading_bar_line(bar_width, ratio);
    let bar = Paragraph::new(Text::from(bar_line)).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(bar, area);
}
