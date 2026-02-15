use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{ProgressKind, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};
use unicode_width::UnicodeWidthStr;

pub fn render(frame: &mut Frame, area: Rect, state: &mut UiState, theme: Theme) {
    let mut lines: Vec<Line> = Vec::new();
    let mut first = true;
    for entry in state.progress.iter() {
        let style = match entry.kind {
            ProgressKind::Info => Style::default().fg(theme.info_text),
            ProgressKind::Success => Style::default().fg(theme.success),
            ProgressKind::Error => Style::default().fg(theme.error),
            ProgressKind::Cancelled => Style::default().fg(theme.active),
            ProgressKind::UserMessage => Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
            ProgressKind::UserMessageSuccess => Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
            ProgressKind::UserMessageError => Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
            ProgressKind::UserMessageCancelled => Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        };
        let style = if entry.active {
            style.fg(theme.active).add_modifier(Modifier::BOLD)
        } else {
            style
        };

        // Add blank separator line between entries
        if !first {
            lines.push(Line::from(Span::raw("")));
        }
        first = false;

        // Add special formatting for user messages
        if matches!(
            entry.kind,
            ProgressKind::UserMessage
                | ProgressKind::UserMessageSuccess
                | ProgressKind::UserMessageError
                | ProgressKind::UserMessageCancelled
        ) {
            lines.extend(format_user_message(&entry.text, style, theme, entry.kind));
        } else {
            lines.extend(markdown_lines(&entry.text, style, theme));
        }
    }
    if state.main_body_follow {
        lines.push(Line::from(Span::raw("")));
    }

    let block = panel_block(theme, theme.surface, PANEL_PADDING);
    let inner = block.inner(area);
    let panel_width = inner.width as usize;
    let panel_height = inner.height as usize;

    let total_height = compute_wrapped_height(&lines, panel_width);
    let max_scroll = total_height.saturating_sub(panel_height);
    state.main_body_max_scroll = max_scroll;

    let clamped = state.main_body_scroll.min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(clamped);

    let progress = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0))
        .block(block);
    frame.render_widget(progress, area);
}

/// Approximate the total number of visual rows the lines will occupy
/// after wrapping to the given panel width.
fn compute_wrapped_height(lines: &[Line], panel_width: usize) -> usize {
    if panel_width == 0 {
        return lines.len();
    }
    lines
        .iter()
        .map(|line| {
            let w: usize = line.spans.iter().map(|s| s.content.width()).sum();
            if w == 0 {
                1 // empty lines still take one row
            } else {
                (w + panel_width - 1) / panel_width // ceil division
            }
        })
        .sum()
}

fn markdown_lines(text: &str, base_style: Style, theme: Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let parser = Parser::new(text);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack = vec![base_style];
    let mut in_code_block = false;
    let mut in_heading = false;
    let code_style = Style::default()
        .fg(theme.active)
        .add_modifier(Modifier::BOLD);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                spans.push(Span::styled("# ", base_style.add_modifier(Modifier::BOLD)));
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                push_line(&mut lines, &mut spans);
            }
            Event::Start(Tag::List(_)) => {}
            Event::End(TagEnd::List(_)) => {}
            Event::Start(Tag::Item) => {
                spans.push(Span::styled("  - ", base_style));
            }
            Event::End(TagEnd::Item) => {
                push_line(&mut lines, &mut spans);
            }
            Event::Rule => {
                push_line(&mut lines, &mut spans);
                lines.push(Line::from(Span::styled(
                    "────────",
                    Style::default().fg(theme.info_text),
                )));
            }
            Event::Start(Tag::Strong) | Event::Start(Tag::Emphasis) => {
                let current = *style_stack.last().unwrap_or(&base_style);
                style_stack.push(current.add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) | Event::End(TagEnd::Emphasis) => {
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                push_line(&mut lines, &mut spans);
            }
            Event::Text(text) => {
                if in_code_block {
                    // Prefix each line with "│ " for visual distinction
                    for (i, code_line) in text.as_ref().lines().enumerate() {
                        if i > 0 {
                            push_line(&mut lines, &mut spans);
                        }
                        spans.push(Span::styled(format!("│ {}", code_line), code_style));
                    }
                } else if in_heading {
                    spans.push(Span::styled(
                        text.into_string(),
                        base_style.add_modifier(Modifier::BOLD),
                    ));
                } else {
                    let style = *style_stack.last().unwrap_or(&base_style);
                    spans.push(Span::styled(text.into_string(), style));
                }
            }
            Event::Code(code) => {
                spans.push(Span::styled(code.into_string(), code_style));
            }
            Event::SoftBreak => {
                spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                push_line(&mut lines, &mut spans);
            }
            Event::End(TagEnd::Paragraph) => {
                push_line(&mut lines, &mut spans);
            }
            _ => {}
        }
    }

    if !spans.is_empty() {
        push_line(&mut lines, &mut spans);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(text.to_string(), base_style)));
    }

    lines
}

fn push_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
    if spans.is_empty() {
        return;
    }
    lines.push(Line::from(spans.drain(..).collect::<Vec<_>>()));
}

fn format_user_message(
    text: &str,
    style: Style,
    theme: Theme,
    kind: ProgressKind,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Choose border color based on request status
    let border_color = match kind {
        ProgressKind::UserMessageSuccess => theme.success,
        ProgressKind::UserMessageError => theme.error,
        ProgressKind::UserMessageCancelled => theme.active,
        _ => ratatui::style::Color::Yellow, // Default for UserMessage (in-progress)
    };
    let border_style = Style::default().fg(border_color);
    let border = "▌ "; // Block character for left border

    // Process each line of the user's message with a colored left border
    for line in text.lines() {
        lines.push(Line::from(vec![
            Span::styled(border, border_style),
            Span::styled(line.to_string(), style),
        ]));
    }

    // Add an empty line after the user message for spacing
    lines.push(Line::from(Span::raw("")));

    lines
}
