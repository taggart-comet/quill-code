use crate::infrastructure::cli::helpers::{bottom_left_aligned_rect, list_state, panel_block};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem};
use ratatui::Frame;
use std::cmp::min;

#[derive(Debug, Clone)]
pub struct CommandsMenuItem {
    pub label: &'static str,
    pub description: &'static str,
}

pub fn commands_items(has_session_id: bool) -> Vec<CommandsMenuItem> {
    let mut items = vec![
        CommandsMenuItem {
            label: "Model",
            description: "Select inference provider and model",
        },
        CommandsMenuItem {
            label: "Mode",
            description: "Switch between Build and Plan (Press Tab)",
        },
        CommandsMenuItem {
            label: "Settings",
            description: "Toggle behavior trees and tracing",
        },
        CommandsMenuItem {
            label: "Continue",
            description: "Resume a previous session",
        },
    ];

    if has_session_id {
        items.push(CommandsMenuItem {
            label: "Compress",
            description: "Compress current session context",
        });
    }

    items.push(CommandsMenuItem {
        label: "Exit",
        description: "Close the application",
    });

    items
}

pub fn render(
    frame: &mut Frame,
    size: Rect,
    selected: usize,
    theme: Theme,
    has_session_id: bool,
) {
    let items = commands_items(has_session_id);
    let height = (items.len() * 2 + 2) as u16;
    let width = min(
        (size.width as f32 * 0.7) as u16,
        size.width.saturating_sub(2),
    );
    let area = bottom_left_aligned_rect(size, width, height);

    frame.render_widget(Clear, area);
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| {
            ListItem::new(vec![
                Line::from(Span::styled(
                    item.label,
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(theme.info_text),
                )),
                Line::from(Span::styled(
                    item.description,
                    Style::default().fg(Color::Rgb(150, 160, 170)),
                )),
            ])
        })
        .collect();

    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )
        .block(panel_block(theme, theme.panel, PANEL_PADDING));

    let mut list_state = list_state(selected);
    frame.render_stateful_widget(list, area, &mut list_state);
}