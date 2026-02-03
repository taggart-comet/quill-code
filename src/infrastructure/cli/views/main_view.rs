use crate::infrastructure::cli::components::{
    attachment_indicator, bottom_info_panel, commands_menu, header_panel, input, left_container,
    loading_bar, permissions, popup_container, request_indicator, right_container, settings_panel,
};
use crate::infrastructure::cli::helpers::{
    centered_rect, cursor_position, list_state, panel_block,
};
use crate::infrastructure::cli::state::{LoadStatus, PopupState, UiMode, UiState};
use crate::infrastructure::cli::theme::{Theme, INPUT_PADDING, PANEL_PADDING};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::cmp::min;

pub fn render(frame: &mut Frame, state: &UiState) {
    let size = frame.size();
    let theme = Theme::new();
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme.background)),
        size,
    );
    let header_height = 3u16;
    let info_height = 3u16;
    let mut input_lines = state.input_line_count();
    input_lines = min(
        input_lines,
        crate::infrastructure::cli::state::INPUT_MAX_HEIGHT,
    );
    let mut input_box_height = (input_lines + INPUT_PADDING.top as usize + INPUT_PADDING.bottom as usize) as u16;

    let indicator_height = 1u16;
    let attachment_indicator_height = if state.attached_images.is_empty() {
        0u16
    } else {
        1u16
    };
    let fixed_height = header_height
        + info_height
        + input_box_height
        + indicator_height
        + attachment_indicator_height;
    if fixed_height > size.height {
        let overflow = fixed_height - size.height;
        let reduced = input_box_height.saturating_sub(overflow);
        input_box_height =
            reduced.max((crate::infrastructure::cli::state::INPUT_MIN_HEIGHT + INPUT_PADDING.top as usize + INPUT_PADDING.bottom as usize) as u16);
    }

    let constraints = if state.attached_images.is_empty() {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(3),
            Constraint::Length(indicator_height),
            Constraint::Length(input_box_height),
            Constraint::Length(info_height),
        ]
    } else {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(3),
            Constraint::Length(indicator_height),
            Constraint::Length(attachment_indicator_height),
            Constraint::Length(input_box_height),
            Constraint::Length(info_height),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    if state.attached_images.is_empty() {
        header_panel::render(frame, chunks[0], state, theme);
        if state.file_changes.is_some() || state.todo_list.is_some() {
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);
            left_container::render(frame, body_chunks[0], state, theme); // Conversation on left
            right_container::render(frame, body_chunks[1], state, theme); // TODO/changes on right
        } else {
            left_container::render(frame, chunks[1], state, theme);
        }
        request_indicator::render(frame, chunks[2], state, theme);
        input::render(frame, chunks[3], state, theme);
        bottom_info_panel::render(frame, chunks[4], state, theme);
    } else {
        header_panel::render(frame, chunks[0], state, theme);
        if state.file_changes.is_some() || state.todo_list.is_some() {
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);
            left_container::render(frame, body_chunks[0], state, theme); // Conversation on left
            right_container::render(frame, body_chunks[1], state, theme); // TODO/changes on right
        } else {
            left_container::render(frame, chunks[1], state, theme);
        }
        request_indicator::render(frame, chunks[2], state, theme);
        attachment_indicator::render(frame, chunks[3], state, theme);
        input::render(frame, chunks[4], state, theme);
        bottom_info_panel::render(frame, chunks[5], state, theme);
    }

    let input_chunk_index = if state.attached_images.is_empty() {
        3
    } else {
        4
    };
    match &state.mode {
        UiMode::CommandsMenu { selected } => commands_menu::render(frame, size, *selected, theme),
        UiMode::Popup(popup) => {
            render_popup(frame, size, chunks[input_chunk_index], state, popup, theme)
        }
        UiMode::Normal | UiMode::FileChangesReview { .. } | UiMode::TodoListReview { .. } => {}
    }
}

fn render_popup(
    frame: &mut Frame,
    size: Rect,
    _input_area: Rect,
    state: &UiState,
    popup: &PopupState,
    theme: Theme,
) {
    match popup {
        PopupState::ModelSelect { selected } => {
            let entries = model_entries(state);
            let height = (entries.len() + 4) as u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let height = height.min(size.height.saturating_sub(2));
            let area = centered_rect(size, width, height);
            frame.render_widget(Clear, area);

            let items: Vec<ListItem> = entries
                .iter()
                .map(|entry| {
                    let label = match entry {
                        ModelEntry::Local { name, .. } => format!("Local: {}", name),
                        ModelEntry::OpenAi { name } => format!("OpenAI: {}", name),
                        ModelEntry::OpenAiSelect => "OpenAI: Select New Model".to_string(),
                        ModelEntry::Loading => "Loading models...".to_string(),
                        ModelEntry::Empty => "No models available".to_string(),
                    };
                    ListItem::new(Line::from(Span::styled(
                        label,
                        Style::default().fg(theme.info_text),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .block(panel_block(theme, theme.panel, PANEL_PADDING))
                .highlight_symbol("> ");
            let mut list_state = list_state(*selected);
            frame.render_stateful_widget(list, area, &mut list_state);
        }
        PopupState::OpenAiAvailable {
            selected,
            filter,
            cursor,
        } => {
            let models = openai_available_entries(state, filter);
            let height = (models.len() + 6) as u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let height = height.min(size.height.saturating_sub(2));
            let area = centered_rect(size, width, height);
            let inner = popup_container::render(frame, area, theme);

            if state.models.openai_available_status == LoadStatus::Loading {
                if inner.height < 3 {
                    let loading = Paragraph::new(Text::from("Loading models..."))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(theme.info_text));
                    frame.render_widget(loading, inner);
                    return;
                }

                let sections = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(2),
                        Constraint::Length(1),
                        Constraint::Min(0),
                    ])
                    .split(inner);

                let title = Paragraph::new(Text::from("Fetching OpenAI models"))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(theme.info_text));
                frame.render_widget(title, sections[0]);

                loading_bar::render(frame, sections[1], state.loading_bar.ratio());
                return;
            }

            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(inner);

            let prefix = "Filter: ";
            let filter_line = format!("{}{}", prefix, filter);
            let filter_text = Paragraph::new(Text::from(filter_line))
                .style(Style::default().fg(theme.info_text))
                .wrap(Wrap { trim: false });
            frame.render_widget(filter_text, sections[0]);

            let items: Vec<ListItem> = models
                .iter()
                .map(|label| {
                    ListItem::new(Line::from(Span::styled(
                        label.clone(),
                        Style::default().fg(theme.info_text),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");
            let mut list_state = list_state(*selected);
            frame.render_stateful_widget(list, sections[1], &mut list_state);

            let (cursor_x, _) = cursor_position(filter, *cursor);
            let x = sections[0]
                .x
                .saturating_add(cursor_x)
                .saturating_add(prefix.len() as u16);
            let y = sections[0].y;
            frame.set_cursor(x, y);
        }
        PopupState::SettingsToggle {
            selected,
            behavior_trees,
            openai_tracing,
            web_search,
            max_tool_calls,
        } => {
            let height = 10u16.min(size.height.saturating_sub(2));
            if height == 0 {
                return;
            }
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let area = centered_rect(size, width, height);
            frame.render_widget(Clear, area);
            settings_panel::render(
                frame,
                area,
                *selected,
                *behavior_trees,
                *openai_tracing,
                *web_search,
                *max_tool_calls,
                theme,
            );
        }
        PopupState::BraveApiKeyPrompt { .. } => {
            let height = 6u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let area = centered_rect(size, width, height);
            let inner = popup_container::render(frame, area, theme);

            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(1)])
                .split(inner);

            let title = Paragraph::new(Text::from("Enter Brave Search API Key"))
                .style(
                    Style::default()
                        .fg(theme.info_text)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            frame.render_widget(title, sections[0]);

            let input_area = sections[1];
            let raw_text = state
                .popup_input
                .as_ref()
                .map(|input| input.text.clone())
                .unwrap_or_default();
            let mask = "***";
            let mask_width = mask.chars().count() as u16;
            let display_text = mask.repeat(raw_text.chars().count());
            let input_widget = Paragraph::new(Text::from(display_text))
                .wrap(Wrap { trim: false })
                .style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .block(panel_block(
                    theme,
                    theme.panel,
                    ratatui::widgets::block::Padding::new(0, 0, 0, 0),
                ));
            frame.render_widget(input_widget, input_area);

            let cursor_x = state
                .popup_input
                .as_ref()
                .map(|input| input.cursor)
                .unwrap_or(0) as u16;
            let x = input_area
                .x
                .saturating_add(cursor_x.saturating_mul(mask_width));
            let y = input_area.y;
            frame.set_cursor(x, y);
        }
        PopupState::OpenAiApiKeyPrompt { .. } => {
            let height = 6u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let area = centered_rect(size, width, height);
            let inner = popup_container::render(frame, area, theme);

            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(1)])
                .split(inner);

            let title = Paragraph::new(Text::from("Enter OpenAI API Key"))
                .style(
                    Style::default()
                        .fg(theme.info_text)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            frame.render_widget(title, sections[0]);

            let input_area = sections[1];
            let raw_text = state
                .popup_input
                .as_ref()
                .map(|input| input.text.clone())
                .unwrap_or_default();
            let mask = "***";
            let mask_width = mask.chars().count() as u16;
            let display_text = mask.repeat(raw_text.chars().count());
            let input_widget = Paragraph::new(Text::from(display_text))
                .wrap(Wrap { trim: false })
                .style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .block(panel_block(
                    theme,
                    theme.panel,
                    ratatui::widgets::block::Padding::new(0, 0, 0, 0),
                ));
            frame.render_widget(input_widget, input_area);

            let cursor_x = state
                .popup_input
                .as_ref()
                .map(|input| input.cursor)
                .unwrap_or(0) as u16;
            let x = input_area
                .x
                .saturating_add(cursor_x.saturating_mul(mask_width));
            let y = input_area.y;
            frame.set_cursor(x, y);
        }
        PopupState::ModeSelect { selected } => {
            let height = 7u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let area = centered_rect(size, width, height);
            frame.render_widget(Clear, area);

            let build = ListItem::new(Line::from(Span::styled(
                "Build",
                Style::default().fg(theme.info_text),
            )));
            let plan = ListItem::new(Line::from(Span::styled(
                "Plan",
                Style::default().fg(theme.info_text),
            )));
            let list = List::new(vec![build, plan])
                .highlight_style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .block(panel_block(theme, theme.panel, PANEL_PADDING))
                .highlight_symbol("> ");
            let mut list_state = list_state(*selected);
            frame.render_stateful_widget(list, area, &mut list_state);
        }
        PopupState::PermissionPrompt {
            tool_name,
            command,
            paths,
            scope,
            selected,
            ..
        } => {
            permissions::render(
                frame,
                size,
                tool_name,
                command.as_deref(),
                paths,
                scope,
                *selected,
                theme,
            );
        }
        PopupState::ContinueSelect { sessions, selected } => {
            if sessions.is_empty() {
                return;
            }
            let height = (sessions.len() + 4) as u16;
            let width = min(
                (size.width as f32 * 0.7) as u16,
                size.width.saturating_sub(2),
            );
            let height = height.min(size.height.saturating_sub(2));
            let area = centered_rect(size, width, height);
            frame.render_widget(Clear, area);

            let inner = area.inner(&ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });

            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(1)])
                .split(inner);

            let title = Paragraph::new(Text::from("Continue Session"))
                .style(
                    Style::default()
                        .fg(theme.info_text)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            frame.render_widget(title, sections[0]);

            let items: Vec<ListItem> = sessions
                .iter()
                .map(|s| {
                    let label = format!("{}  ({})", s.name, s.created_at);
                    ListItem::new(Line::from(Span::styled(
                        label,
                        Style::default().fg(theme.info_text),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(theme.active)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");
            let mut list_state = list_state(*selected);
            frame.render_stateful_widget(list, sections[1], &mut list_state);
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModelEntry {
    Local { name: String, path: String },
    OpenAi { name: String },
    OpenAiSelect,
    Loading,
    Empty,
}

pub fn model_entries(state: &UiState) -> Vec<ModelEntry> {
    if state.models.status == LoadStatus::Loading {
        return vec![ModelEntry::Loading];
    }

    let mut entries = Vec::new();
    for model in &state.models.local {
        entries.push(ModelEntry::Local {
            name: model.name.clone(),
            path: model.path.clone(),
        });
    }
    for model in &state.models.openai {
        entries.push(ModelEntry::OpenAi {
            name: model.name.clone(),
        });
    }
    entries.push(ModelEntry::OpenAiSelect);

    if entries.is_empty() {
        vec![ModelEntry::Empty]
    } else {
        entries
    }
}

pub fn openai_available_entries(state: &UiState, filter: &str) -> Vec<String> {
    if state.models.openai_available_status == LoadStatus::Loading {
        return vec!["Loading models...".to_string()];
    }
    if state.models.openai_available.is_empty() {
        return vec!["No models available".to_string()];
    }
    let filtered = openai_available_filtered(state, filter);
    if filtered.is_empty() {
        vec!["No matches found".to_string()]
    } else {
        filtered
    }
}

pub fn openai_available_filtered(state: &UiState, filter: &str) -> Vec<String> {
    if state.models.openai_available_status != LoadStatus::Loaded {
        return Vec::new();
    }
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        return state.models.openai_available.clone();
    }
    let query = trimmed.to_lowercase();
    state
        .models
        .openai_available
        .iter()
        .filter(|name| name.to_lowercase().contains(&query))
        .cloned()
        .collect()
}