use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{ProgressKind, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};

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
        lines.extend(markdown_lines(&entry.text, style, theme));
    }

    let scroll_offset = state.main_body_scroll;
    let start = lines
        .len()
        .saturating_sub(available.saturating_add(scroll_offset));
    let end = (start + available).min(lines.len());
    let visible = lines[start..end].to_vec();
    let progress = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(progress, area);
}

fn markdown_lines(text: &str, base_style: Style, theme: Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let parser = Parser::new(text);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack = vec![base_style];
    let mut in_code_block = false;
    let code_style = Style::default()
        .fg(theme.active)
        .add_modifier(Modifier::BOLD);

    for event in parser {
        match event {
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
                let style = if in_code_block {
                    code_style
                } else {
                    *style_stack.last().unwrap_or(&base_style)
                };
                spans.push(Span::styled(text.into_string(), style));
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
