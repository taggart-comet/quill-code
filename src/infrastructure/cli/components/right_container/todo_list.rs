use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{
    TodoItemDisplay, TodoListDisplay, TodoListViewMode, UiMode, UiState,
};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let Some(todo_list) = state.todo_list.as_ref() else {
        return;
    };

    // Extract review mode state
    let (selected_item, view_mode, scroll_offset) = match &state.mode {
        UiMode::TodoListReview {
            selected_item,
            view_mode,
            scroll_offset,
        } => (Some(*selected_item), Some(view_mode), *scroll_offset),
        _ => (None, None, 0),
    };

    // Render based on mode
    if let Some(TodoListViewMode::ItemDetail) = view_mode {
        // Show item detail for selected item
        if let Some(selected_idx) = selected_item {
            if let Some(item) = todo_list.items.get(selected_idx) {
                render_item_detail(frame, area, item, scroll_offset, theme);
            }
        }
    } else {
        // Show item list (either static or interactive)
        render_item_list(frame, area, todo_list, selected_item, theme);
    }
}

fn render_item_list(
    frame: &mut Frame,
    area: Rect,
    todo_list: &TodoListDisplay,
    selected: Option<usize>,
    theme: Theme,
) {
    let header = Line::from(vec![
        Span::styled(
            "TODO List",
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({} items)", todo_list.items.len()),
            Style::default().fg(theme.border),
        ),
    ]);

    let mut lines: Vec<Line> = vec![header];

    for (idx, item) in todo_list.items.iter().enumerate() {
        let is_selected = selected == Some(idx);

        let status_icon = match item.status.as_str() {
            "completed" => "✓",
            "in_progress" => "⋯",
            _ => "○",
        };

        let status_color = match item.status.as_str() {
            "completed" => theme.success,
            "in_progress" => theme.active,
            _ => theme.border,
        };

        let prefix = if is_selected { "> " } else { "  " };

        if is_selected {
            // Highlighted selection
            lines.push(Line::from(vec![Span::styled(
                format!("{}{} {}", prefix, status_icon, item.title),
                Style::default()
                    .fg(theme.background)
                    .bg(theme.active)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", status_icon),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    &item.title,
                    Style::default()
                        .fg(theme.info_text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}

fn render_item_detail(
    frame: &mut Frame,
    area: Rect,
    item: &TodoItemDisplay,
    scroll_offset: usize,
    theme: Theme,
) {
    let available = area
        .height
        .saturating_sub(PANEL_PADDING.top + PANEL_PADDING.bottom) as usize;
    if available == 0 {
        return;
    }

    let status_icon = match item.status.as_str() {
        "completed" => "✓",
        "in_progress" => "⋯",
        _ => "○",
    };

    let status_color = match item.status.as_str() {
        "completed" => theme.success,
        "in_progress" => theme.active,
        _ => theme.border,
    };

    let header = Line::from(vec![
        Span::styled(
            format!("{} ", status_icon),
            Style::default().fg(status_color),
        ),
        Span::styled(
            format!("{} ", &item.title),
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("(Press Esc to return)", Style::default().fg(theme.border)),
    ]);

    let mut lines: Vec<Line> = vec![header, Line::from("")];

    // Parse description and apply scrolling
    let description_lines: Vec<&str> = item.description.lines().collect();
    let total_lines = description_lines.len();

    // Calculate which lines to show based on scroll_offset
    let start = scroll_offset.min(total_lines.saturating_sub(1));
    let end = (start + available.saturating_sub(2)).min(total_lines); // -2 for header and blank line

    for line in &description_lines[start..end] {
        lines.push(Line::from(Span::styled(
            *line,
            Style::default().fg(theme.info_text),
        )));
    }

    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}
