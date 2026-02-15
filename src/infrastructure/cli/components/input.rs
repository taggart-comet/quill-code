use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{UiMode, UiState};
use crate::infrastructure::cli::theme::{Theme, INPUT_PADDING};
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};
use unicode_width::UnicodeWidthChar;

struct WrappedLine {
    text: String,
    source_row: usize,
    start_col: usize,
    end_col: usize,
}

fn wrap_lines(lines: &[String], width: usize) -> Vec<WrappedLine> {
    let width = width.max(1);
    let mut wrapped = Vec::new();

    for (row, line) in lines.iter().enumerate() {
        if line.is_empty() {
            wrapped.push(WrappedLine {
                text: String::new(),
                source_row: row,
                start_col: 0,
                end_col: 0,
            });
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0usize;
        let mut start_col = 0usize;
        let mut current_cols = 0usize;

        for (idx, ch) in line.chars().enumerate() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + ch_width > width && !current.is_empty() {
                let end_col = start_col + current_cols;
                wrapped.push(WrappedLine {
                    text: current,
                    source_row: row,
                    start_col,
                    end_col,
                });
                current = String::new();
                current_width = 0;
                current_cols = 0;
                start_col = idx;
            }

            current.push(ch);
            current_width += ch_width;
            current_cols += 1;
        }

        let end_col = start_col + current_cols;
        wrapped.push(WrappedLine {
            text: current,
            source_row: row,
            start_col,
            end_col,
        });
    }

    if wrapped.is_empty() {
        wrapped.push(WrappedLine {
            text: String::new(),
            source_row: 0,
            start_col: 0,
            end_col: 0,
        });
    }

    wrapped
}

fn cursor_visual_position(
    lines: &[String],
    wrapped_lines: &[WrappedLine],
    cursor_row: usize,
    cursor_col: usize,
) -> (usize, usize) {
    if lines.is_empty() || wrapped_lines.is_empty() {
        return (0, 0);
    }

    let mut fallback_row = 0usize;
    for (idx, wrapped) in wrapped_lines.iter().enumerate() {
        if wrapped.source_row == cursor_row {
            fallback_row = idx;
            if cursor_col <= wrapped.end_col || wrapped.start_col == wrapped.end_col {
                let line = &lines[cursor_row];
                let slice_width: usize = line
                    .chars()
                    .skip(wrapped.start_col)
                    .take(cursor_col.saturating_sub(wrapped.start_col))
                    .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
                    .sum();
                return (idx, slice_width);
            }
        }
    }

    let fallback_line = String::new();
    let line = lines.get(cursor_row).unwrap_or(&fallback_line);
    let slice_width: usize = line
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum();
    (fallback_row, slice_width)
}

pub fn visual_line_count(state: &UiState, area_width: u16) -> usize {
    let text_width = area_width
        .saturating_sub(INPUT_PADDING.left + INPUT_PADDING.right) as usize;
    let lines: Vec<String> = state.input.lines().to_vec();
    wrap_lines(&lines, text_width).len().max(1)
}

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let visible_lines = area
        .height
        .saturating_sub(INPUT_PADDING.top + INPUT_PADDING.bottom) as usize;
    let text_width = area
        .width
        .saturating_sub(INPUT_PADDING.left + INPUT_PADDING.right) as usize;
    let lines: Vec<String> = state.input.lines().to_vec();
    let wrapped_lines = wrap_lines(&lines, text_width);
    let total_lines = wrapped_lines.len().max(1);
    let (cursor_row, cursor_col) = state.input.cursor();
    let (cursor_line, cursor_col_width) =
        cursor_visual_position(&lines, &wrapped_lines, cursor_row as usize, cursor_col as usize);
    let desired_center = visible_lines / 2;
    let desired_offset = cursor_line.saturating_sub(desired_center);
    let max_offset = total_lines.saturating_sub(visible_lines);
    let offset = desired_offset.min(max_offset);
    let viewport_cursor_line = cursor_line.saturating_sub(offset);
    let top_padding = desired_center.saturating_sub(viewport_cursor_line);
    let display_text = if wrapped_lines.is_empty() {
        String::new()
    } else {
        let end = (offset + visible_lines).min(total_lines);
        wrapped_lines[offset..end]
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let display_text = if top_padding > 0 {
        let mut padded = String::new();
        padded.push_str(&"\n".repeat(top_padding));
        padded.push_str(&display_text);
        padded
    } else {
        display_text
    };

    let input = Paragraph::new(Text::from(display_text))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.info_text))
        .block(panel_block(theme, theme.panel, INPUT_PADDING));
    frame.render_widget(input, area);

    if matches!(state.mode, UiMode::Normal) {
        let cursor_x = cursor_col_width as u16;
        let cursor_y = cursor_line as u16;
        let adjusted_line = cursor_y
            .saturating_sub(offset as u16)
            .saturating_add(top_padding as u16);
        let x = area.x.saturating_add(INPUT_PADDING.left + cursor_x);
        let y = area.y.saturating_add(INPUT_PADDING.top + adjusted_line);
        frame.set_cursor(x, y);
    }
}