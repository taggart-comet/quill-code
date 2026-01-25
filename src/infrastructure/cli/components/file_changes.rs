use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let Some(file_changes) = state.file_changes.as_ref() else {
        return;
    };

    let available = area
        .height
        .saturating_sub(PANEL_PADDING.top + PANEL_PADDING.bottom) as usize;
    if available == 0 {
        return;
    }

    let header = Line::from(vec![
        Span::styled(
            "File changes",
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" (request {})", file_changes.request_id),
            Style::default().fg(theme.border),
        ),
    ]);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(header);

    for change in &file_changes.changes {
        lines.push(Line::from(vec![
            Span::styled(change.path.clone(), Style::default().fg(theme.info_text)),
            Span::styled(": ", Style::default().fg(theme.info_text)),
            Span::styled(
                format!("+{}", change.added_lines),
                Style::default().fg(theme.success),
            ),
            Span::styled(" ", Style::default().fg(theme.info_text)),
            Span::styled(
                format!("-{}", change.deleted_lines),
                Style::default().fg(theme.error),
            ),
        ]));
    }

    let visible = if lines.len() <= available {
        lines
    } else {
        let tail_len = available.saturating_sub(1);
        let start = lines.len().saturating_sub(tail_len);
        let mut trimmed = Vec::with_capacity(available);
        trimmed.push(lines[0].clone());
        trimmed.extend_from_slice(&lines[start..]);
        trimmed
    };

    let panel = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}
