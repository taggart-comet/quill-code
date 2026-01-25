use crate::infrastructure::cli::helpers::widgets::loading_bar_line_with_colors;
use crate::infrastructure::cli::state::UiState;
use crate::infrastructure::cli::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    if area.height == 0 {
        return;
    }

    let ratio = if state.request_status.is_some() {
        1.0
    } else if state.request_in_flight.is_some() {
        state.loading_bar.ratio()
    } else {
        return;
    };
    let active_color = theme.success;

    let bar_width = if area.width < 20 {
        area.width.max(6) as usize
    } else {
        let base = (area.width / 7).max(8).min(14) as f64;
        (base * 0.75).round().max(6.0) as usize
    };
    let bar_line =
        loading_bar_line_with_colors(bar_width, ratio, active_color, inactive_color(theme));

    let message = state.request_progress.clone().unwrap_or_default();
    let glow = glow_line(&message, ratio, theme.info_text);
    let line = merge_lines(bar_line, glow);
    let status = Paragraph::new(Text::from(line))
        .alignment(Alignment::Left)
        .style(Style::default().fg(theme.info_text));
    frame.render_widget(status, area);
}

fn glow_line(message: &str, ratio: f64, base: Color) -> Line<'static> {
    if message.is_empty() {
        return Line::from(Span::raw(""));
    }
    let chars: Vec<char> = message.chars().collect();
    let len = chars.len();
    if len == 0 {
        return Line::from(Span::raw(""));
    }
    let raw_index = (ratio * len as f64).floor() as usize;
    let start = raw_index.min(len.saturating_sub(1));
    let glow_mid = Color::Rgb(255, 255, 255);
    let glow_edge = Color::Rgb(210, 210, 210);
    let highlight = [start, start + 1, start + 2];

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_style = Style::default().fg(base);

    for (idx, ch) in chars.iter().enumerate() {
        let style = if highlight.contains(&idx) {
            let glow_style = if idx == start + 1 {
                Style::default().fg(glow_mid)
            } else {
                Style::default().fg(glow_edge)
            };
            glow_style.add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base)
        };

        if style == current_style {
            current.push(*ch);
        } else {
            if !current.is_empty() {
                spans.push(Span::styled(current.clone(), current_style));
                current.clear();
            }
            current_style = style;
            current.push(*ch);
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, current_style));
    }

    Line::from(spans)
}

fn inactive_color(theme: Theme) -> Color {
    theme.border
}

fn merge_lines(mut left: Line<'static>, right: Line<'static>) -> Line<'static> {
    left.spans.push(Span::raw(" "));
    left.spans.extend(right.spans);
    left
}
