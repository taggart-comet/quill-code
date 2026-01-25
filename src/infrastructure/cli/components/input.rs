use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{UiMode, UiState};
use crate::infrastructure::cli::theme::{Theme, INPUT_PADDING};
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let visible_lines = area
        .height
        .saturating_sub(INPUT_PADDING.top + INPUT_PADDING.bottom) as usize;
    let lines: Vec<String> = state.input.lines().to_vec();
    let total_lines = lines.len().max(1);
    let (cursor_row, cursor_col) = state.input.cursor();
    let cursor_line = cursor_row as usize;
    let desired_center = visible_lines / 2;
    let desired_offset = cursor_line.saturating_sub(desired_center);
    let max_offset = total_lines.saturating_sub(visible_lines);
    let offset = desired_offset.min(max_offset);
    let viewport_cursor_line = cursor_line.saturating_sub(offset);
    let top_padding = desired_center.saturating_sub(viewport_cursor_line);
    let display_text = if lines.is_empty() {
        String::new()
    } else {
        let end = (offset + visible_lines).min(total_lines);
        lines[offset..end].join("\n")
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
        let cursor_x = cursor_col as u16;
        let cursor_y = cursor_row as u16;
        let adjusted_line = cursor_y
            .saturating_sub(offset as u16)
            .saturating_add(top_padding as u16);
        let x = area.x.saturating_add(INPUT_PADDING.left + cursor_x);
        let y = area.y.saturating_add(INPUT_PADDING.top + adjusted_line);
        frame.set_cursor(x, y);
    }
}
