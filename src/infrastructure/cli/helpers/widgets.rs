use crate::infrastructure::cli::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::block::Padding;
use ratatui::widgets::{Block, Borders, ListItem};

pub fn panel_block(theme: Theme, background: Color, padding: Padding) -> Block<'static> {
    Block::default()
        .borders(Borders::NONE)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(background))
        .padding(padding)
}

pub fn cursor_position(text: &str, cursor: usize) -> (u16, u16) {
    let mut line = 0u16;
    let mut col = 0u16;
    for (idx, ch) in text.char_indices() {
        if idx >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (col, line)
}

pub fn loading_bar_line(width: usize, ratio: f64) -> Line<'static> {
    loading_bar_line_with_colors(
        width,
        ratio,
        Color::Rgb(130, 160, 180),
        Color::Rgb(70, 78, 90),
    )
}

pub fn loading_bar_line_with_colors(
    width: usize,
    ratio: f64,
    active_color: Color,
    inactive_color: Color,
) -> Line<'static> {
    let bar_width = width.max(6);
    let segment_width = (bar_width / 4).max(3).min(bar_width);
    let max_offset = bar_width.saturating_sub(segment_width);
    let offset = ((max_offset as f64) * ratio).round() as usize;
    let mut spans = Vec::new();
    spans.push(Span::raw("["));
    if offset > 0 {
        spans.push(Span::styled(
            " ".repeat(offset),
            Style::default().fg(inactive_color),
        ));
    }
    spans.push(Span::styled(
        "=".repeat(segment_width),
        Style::default().fg(active_color).add_modifier(Modifier::BOLD),
    ));
    let remaining = bar_width.saturating_sub(offset + segment_width);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().fg(inactive_color),
        ));
    }
    spans.push(Span::raw("]"));
    Line::from(spans)
}

pub fn insert_char(text: &mut String, cursor: &mut usize, ch: char) {
    text.insert(*cursor, ch);
    *cursor += ch.len_utf8();
}

pub fn delete_prev_char(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let prev = prev_char_boundary(text, *cursor);
    text.drain(prev..*cursor);
    *cursor = prev;
}

pub fn delete_next_char(text: &mut String, cursor: &mut usize) {
    if *cursor >= text.len() {
        return;
    }
    let next = next_char_boundary(text, *cursor);
    text.drain(*cursor..next);
}

pub fn prev_char_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }
    text[cursor..]
        .char_indices()
        .nth(1)
        .map(|(idx, _)| cursor + idx)
        .unwrap_or(text.len())
}

pub fn list_state(selected: usize) -> ratatui::widgets::ListState {
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));
    state
}

pub fn checkbox_item(label: &str, checked: bool) -> ListItem<'_> {
    let box_text = if checked { "[x]" } else { "[ ]" };
    ListItem::new(Line::from(Span::raw(format!("{} {}", box_text, label))))
}
