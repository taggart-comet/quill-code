use crate::domain::tools::FileChange;
use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{FileChangesDisplay, FileChangesViewMode, UiMode, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let Some(file_changes) = state.file_changes.as_ref() else {
        return;
    };

    // Extract review mode state
    let (selected_file, view_mode, scroll_offset) = match &state.mode {
        UiMode::FileChangesReview {
            selected_file,
            view_mode,
            scroll_offset,
        } => (Some(*selected_file), Some(view_mode), *scroll_offset),
        _ => (None, None, 0),
    };

    // Render based on mode
    if let Some(FileChangesViewMode::UnifiedDiff) = view_mode {
        // Show unified diff for selected file
        if let Some(selected_idx) = selected_file {
            if let Some(change) = file_changes.changes.get(selected_idx) {
                render_unified_diff(frame, area, change, scroll_offset, theme);
            }
        }
    } else {
        // Show file list (either static or interactive)
        render_file_list(frame, area, file_changes, selected_file, theme);
    }
}

fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    file_changes: &FileChangesDisplay,
    selected: Option<usize>,
    theme: Theme,
) {
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

    for (idx, change) in file_changes.changes.iter().enumerate() {
        let is_selected = selected == Some(idx);

        let prefix = if is_selected { "> " } else { "  " };
        let path_text = format!("{}{}", prefix, change.path);

        if is_selected {
            // Highlighted selection
            lines.push(Line::from(vec![
                Span::styled(
                    path_text,
                    Style::default()
                        .fg(theme.background)
                        .bg(theme.active)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(": +{} -{}", change.added_lines, change.deleted_lines),
                    Style::default().fg(theme.background).bg(theme.active),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(path_text, Style::default().fg(theme.info_text)),
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

fn render_unified_diff(
    frame: &mut Frame,
    area: Rect,
    change: &FileChange,
    scroll_offset: usize,
    theme: Theme,
) {
    let available = area
        .height
        .saturating_sub(PANEL_PADDING.top + PANEL_PADDING.bottom) as usize;
    if available == 0 {
        return;
    }

    let header = Line::from(vec![
        Span::styled(
            format!("Diff: {} ", change.path),
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("(Press Esc to return)", Style::default().fg(theme.border)),
    ]);

    let mut lines: Vec<Line> = vec![header];

    let filtered = filter_diff_lines(&change.unified_diff, 5);
    let total_lines = filtered.len();

    // Calculate which lines to show based on scroll_offset
    let start = scroll_offset.min(total_lines.saturating_sub(1));
    let end = (start + available.saturating_sub(1)).min(total_lines);

    for line in &filtered[start..end] {
        let styled_line = if line.starts_with("+++") || line.starts_with("---") {
            // File headers
            Line::from(Span::styled(
                line.as_str(),
                Style::default()
                    .fg(theme.border)
                    .add_modifier(Modifier::BOLD),
            ))
        } else if line.starts_with("@@") {
            // Hunk headers
            Line::from(Span::styled(
                line.as_str(),
                Style::default()
                    .fg(theme.active)
                    .add_modifier(Modifier::BOLD),
            ))
        } else if line.starts_with('+') {
            // Additions
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme.success).bg(Color::Rgb(0, 40, 0)),
            ))
        } else if line.starts_with('-') {
            // Deletions
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme.error).bg(Color::Rgb(40, 0, 0)),
            ))
        } else {
            // Context lines
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme.info_text),
            ))
        };

        lines.push(styled_line);
    }

    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}

pub(crate) fn filter_diff_lines(unified_diff: &str, context: usize) -> Vec<String> {
    let diff_lines: Vec<&str> = unified_diff.lines().collect();
    let total_lines = diff_lines.len();
    if total_lines == 0 {
        return Vec::new();
    }

    let mut include = vec![false; total_lines];
    for (idx, line) in diff_lines.iter().enumerate() {
        let is_file_header = line.starts_with("+++") || line.starts_with("---");
        let is_hunk_header = line.starts_with("@@");
        if is_file_header || is_hunk_header {
            include[idx] = true;
            continue;
        }

        let is_change = line.starts_with('+') || line.starts_with('-');
        if is_change {
            let start = idx.saturating_sub(context);
            let end = (idx + context).min(total_lines.saturating_sub(1));
            for j in start..=end {
                include[j] = true;
            }
        }
    }

    diff_lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            if include[idx] {
                Some(line.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_diff_lines;

    #[test]
    fn filter_diff_lines_returns_empty_for_empty_input() {
        let filtered = filter_diff_lines("", 2);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_diff_lines_keeps_context_around_changes() {
        let diff = r#"--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,6 +1,6 @@
 fn demo() {
     let a = 1;
-    let b = 2;
+    let b = 3;
     let c = 4;
     let d = 5;
     let e = 6;
 }
"#;

        let filtered = filter_diff_lines(diff, 1);
        assert!(filtered.iter().any(|line| line.contains("let b = 3")));
        assert!(filtered.iter().any(|line| line.contains("let b = 2")));
        assert!(filtered.iter().any(|line| line.contains("let a = 1")));
        assert!(filtered.iter().any(|line| line.contains("let c = 4")));
        assert!(filtered.iter().any(|line| line.contains("@@")));
    }
}
