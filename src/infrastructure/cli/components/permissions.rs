use crate::infrastructure::cli::helpers::{centered_rect, list_state, panel_block};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::cmp::min;

pub fn render(
    frame: &mut Frame,
    size: Rect,
    tool_name: &str,
    command: Option<&str>,
    paths: &[String],
    scope: &str,
    selected: usize,
    theme: Theme,
    is_read_only: bool,
) {
    let width = min(
        (size.width as f32 * 0.7) as u16,
        size.width.saturating_sub(2),
    );
    let height = 12u16.min(size.height.saturating_sub(2));
    let area = centered_rect(size, width, height);
    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(vec![Span::styled(
            "PERMISSION REQUIRED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(Span::raw(format!("Tool: {}", tool_name))),
    ];
    if let Some(command) = command {
        lines.push(Line::from(Span::raw(format!("Command: {}", command))));
    }
    if !paths.is_empty() {
        lines.push(Line::from(Span::raw("Resources:")));
        for path in paths {
            lines.push(Line::from(Span::raw(format!("  {}", path))));
        }
    }
    lines.push(Line::from(Span::raw(format!("Scope: {}", scope))));

    let info = Paragraph::new(lines)
        .style(Style::default().fg(theme.info_text))
        .block(panel_block(theme, theme.panel, PANEL_PADDING))
        .wrap(Wrap { trim: false });
    frame.render_widget(info, area);

    let options_area = Rect {
        x: area.x,
        y: area.y.saturating_add(area.height.saturating_sub(5)),
        width: area.width,
        height: 5,
    };
    let options = permission_options(command.is_some(), is_read_only);
    let items: Vec<ListItem> = options
        .iter()
        .map(|option| ListItem::new(Line::from(Span::raw(*option))))
        .collect();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )
        .block(panel_block(theme, theme.panel, PANEL_PADDING))
        .highlight_symbol("> ");
    let mut list_state = list_state(selected);
    frame.render_stateful_widget(list, options_area, &mut list_state);
}

fn permission_options(has_command: bool, is_read_only: bool) -> Vec<&'static str> {
    if has_command {
        vec![
            "[A] Allow",
            "[C] Allow this Command for this Project",
            "[D] Disallow",
        ]
    } else if is_read_only {
        vec![
            "[A] Allow",
            "[R] Allow All Reads in Project for this Session",
            "[D] Disallow",
        ]
    } else {
        vec![
            "[A] Allow",
            "[W] Allow All Writes in Project for this Session",
            "[D] Disallow",
        ]
    }
}

pub fn option_count() -> usize {
    3
}
