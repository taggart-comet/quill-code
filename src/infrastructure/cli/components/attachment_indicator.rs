use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::time::Duration;

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 60 * 60 {
        format!("{}m ago", seconds / 60)
    } else {
        format!("{}h ago", seconds / 3600)
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, _theme: Theme) {
    if state.attached_images.is_empty() {
        return;
    }

    let count = state.attached_images.len();
    let newest_attachment = state.attached_images.last();
    let attached_ago = newest_attachment
        .map(|img| format_elapsed(img.attached_at.elapsed()))
        .unwrap_or_else(|| "just now".to_string());
    let total_size: usize = state.attached_images.iter().map(|img| img.data.len()).sum();
    let size_str = crate::infrastructure::cli::format_size(total_size);

    let text = if count == 1 {
        let image = &state.attached_images[0];
        format!(
            "📎 {} image attached ({} • {}) • Press Esc to remove",
            count, image.size, attached_ago
        )
    } else {
        format!(
            "📎 {} images attached ({} • {}) • Press Esc to remove all",
            count, size_str, attached_ago
        )
    };

    let indicator = Paragraph::new(Line::from(vec![Span::styled(
        text,
        Style::default().fg(Color::Yellow),
    )]));

    frame.render_widget(indicator, area);
}
