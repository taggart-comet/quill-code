use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{ProgressKind, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{Frame, layout::Rect};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let available = area
        .height
        .saturating_sub(PANEL_PADDING.top + PANEL_PADDING.bottom) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for entry in state.progress.iter() {
        let style = match entry.kind {
            ProgressKind::Info => Style::default().fg(theme.info_text),
            ProgressKind::Success => Style::default().fg(theme.success),
            ProgressKind::Error => Style::default().fg(theme.error),
            ProgressKind::Cancelled => Style::default().fg(theme.active),
        };
        let style = if entry.active {
            style.fg(theme.active).add_modifier(Modifier::BOLD)
        } else {
            style
        };
        lines.push(Line::from(Span::styled(entry.text.clone(), style)));
    }

    let start = lines.len().saturating_sub(available);
    let visible = lines[start..].to_vec();
    let progress = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(progress, area);
}
