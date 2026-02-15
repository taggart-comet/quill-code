use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{ProgressKind, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};
use unicode_width::UnicodeWidthStr;

pub fn render(frame: &mut Frame, area: Rect, state: &mut UiState, theme: Theme) {
    let block = panel_block(theme, theme.surface, PANEL_PADDING);
    let inner = block.inner(area);
    let panel_width = inner.width as usize;
    let panel_height = inner.height as usize;

    // 1. Build logical lines from all progress entries
    let mut logical_lines: Vec<Line> = Vec::new();
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

        if !first {
            logical_lines.push(Line::from(Span::raw("")));
        }
        first = false;

        if matches!(
            entry.kind,
            ProgressKind::UserMessage
                | ProgressKind::UserMessageSuccess
                | ProgressKind::UserMessageError
                | ProgressKind::UserMessageCancelled
        ) {
            logical_lines.extend(format_user_message(&entry.text, style, theme, entry.kind));
        } else {
            logical_lines.extend(markdown_lines(&entry.text, style, theme));
        }
    }
    if state.main_body_follow {
        logical_lines.push(Line::from(Span::raw("")));
    }

    // 2. Pre-wrap: split every logical line into visual rows that each fit panel_width.
    //    After this, rows.len() == exact number of visual rows. No Wrap needed.
    let rows = wrap_lines(logical_lines, panel_width);

    // 3. Slice visible window
    let total = rows.len();
    let max_scroll = total.saturating_sub(panel_height);
    state.main_body_max_scroll = max_scroll;
    let clamped = state.main_body_scroll.min(max_scroll);
    // scroll=0 means "pinned to bottom", higher means further up
    let end = total.saturating_sub(clamped);
    let start = end.saturating_sub(panel_height);
    let visible: Vec<Line> = rows[start..end].to_vec();

    let progress = Paragraph::new(visible).block(block);
    frame.render_widget(progress, area);
}

/// Split each Line into one or more Lines that fit within `max_width` columns.
/// Splits happen at span boundaries when possible; long spans are split mid-text
/// on character boundaries (word-boundary splitting is not attempted for simplicity).
fn wrap_lines(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return lines;
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    for line in lines {
        let total_w: usize = line.spans.iter().map(|s| s.content.width()).sum();
        if total_w <= max_width {
            out.push(line);
            continue;
        }
        // Need to split this line's spans across multiple rows
        let mut row_spans: Vec<Span<'static>> = Vec::new();
        let mut row_w: usize = 0;
        for span in line.spans {
            let span_w = span.content.width();
            if row_w + span_w <= max_width {
                row_w += span_w;
                row_spans.push(span);
                continue;
            }
            // This span doesn't fit entirely — split character by character
            let style = span.style;
            let mut buf = String::new();
            let mut buf_w: usize = 0;
            for ch in span.content.chars() {
                let cw = UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]) as &str);
                if row_w + buf_w + cw > max_width && (row_w + buf_w) > 0 {
                    // Flush buf into current row, then emit the row
                    if !buf.is_empty() {
                        row_spans.push(Span::styled(buf.clone(), style));
                        buf.clear();
                        buf_w = 0;
                    }
                    out.push(Line::from(
                        row_spans.drain(..).collect::<Vec<_>>(),
                    ));
                    row_w = 0;
                }
                buf.push(ch);
                buf_w += cw;
            }
            if !buf.is_empty() {
                row_w += buf_w;
                row_spans.push(Span::styled(buf, style));
            }
        }
        if !row_spans.is_empty() {
            out.push(Line::from(row_spans));
        }
    }
    out
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

    let border_color = match kind {
        ProgressKind::UserMessageSuccess => theme.success,
        ProgressKind::UserMessageError => theme.error,
        ProgressKind::UserMessageCancelled => theme.active,
        _ => ratatui::style::Color::Yellow,
    };
    let border_style = Style::default().fg(border_color);
    let border = "▌ ";

    for line in text.lines() {
        lines.push(Line::from(vec![
            Span::styled(border, border_style),
            Span::styled(line.to_string(), style),
        ]));
    }

    lines.push(Line::from(Span::raw("")));

    lines
}
