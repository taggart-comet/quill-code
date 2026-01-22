use crate::domain::permissions::PermissionDecision;
use crate::infrastructure::app_bus::{EventBus, UiToAgentEvent};
use crate::infrastructure::cli::helpers::{
    delete_next_char, delete_prev_char, insert_char, next_char_boundary, prev_char_boundary,
};
use crate::infrastructure::cli::state::{LoadStatus, PopupInput, PopupState, UiMode, UiState};
use crate::infrastructure::cli::actions::{
    change_settings, insert_openai_key, select_openai_model,
};
use crate::infrastructure::cli::components::{commands_menu, permissions};
use crate::infrastructure::cli::views::main_view::{
    model_entries, openai_available_entries, openai_available_filtered,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rusqlite::Connection;

pub fn handle_key_event(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    key: KeyEvent,
) -> Result<(), String> {
    let mode_snapshot = state.mode.clone();
    match mode_snapshot {
        UiMode::Normal => handle_normal_key(bus, conn, state, key),
        UiMode::CommandsMenu { mut selected } => {
            handle_commands_key(bus, conn, state, key, &mut selected)?;
            if matches!(state.mode, UiMode::CommandsMenu { .. }) {
                state.mode = UiMode::CommandsMenu { selected };
            }
            Ok(())
        }
        UiMode::Popup(mut popup) => {
            handle_popup_key(bus, conn, state, key, &mut popup)?;
            if let UiMode::Popup(current_popup) = &state.mode {
                if std::mem::discriminant(current_popup) == std::mem::discriminant(&popup) {
                    state.mode = UiMode::Popup(popup);
                }
            }
            Ok(())
        }
    }
}

fn handle_normal_key(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    key: KeyEvent,
) -> Result<(), String> {
    match key.code {
        KeyCode::Char('/') if input_is_empty(state) => {
            state.input.input(key);
            state.mode = UiMode::CommandsMenu { selected: 0 };
        }
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::CONTROL)
            {
                state.input.input(key);
            } else {
                submit_input(bus, conn, state)?;
            }
        }
        KeyCode::Char('\n') | KeyCode::Char('\r') => {
            state.input.insert_newline();
        }
        _ => {
            state.input.input(key);
        }
    }

    Ok(())
}

fn handle_commands_key(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    key: KeyEvent,
    selected: &mut usize,
) -> Result<(), String> {
    let item_count = commands_menu::commands_items().len();
    match key.code {
        KeyCode::Esc => state.mode = UiMode::Normal,
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
        }
        KeyCode::Down => {
            if *selected + 1 < item_count {
                *selected += 1;
            }
        }
        KeyCode::Char(_) => {
            state.mode = UiMode::Normal;
            state.input.input(key);
        }
        KeyCode::Enter => match *selected {
            0 => select_openai_model::open_model_popup(conn, state),
            1 => open_mode_popup(state),
            2 => change_settings::open_settings_popup(conn, state),
            3 => request_exit(bus, state),
            _ => state.mode = UiMode::Normal,
        },
        _ => {}
    }

    Ok(())
}

fn handle_popup_key(
    bus: &EventBus,
    conn: &Connection,
    state: &mut UiState,
    key: KeyEvent,
    popup: &mut PopupState,
) -> Result<(), String> {
    match popup {
        PopupState::ModelSelect { selected } => {
            let entries = model_entries(state);
            let count = entries.len();
            match key.code {
                KeyCode::Esc => state.mode = UiMode::Normal,
                KeyCode::Up => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if *selected + 1 < count {
                        *selected += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(entry) = entries.get(*selected) {
                        select_openai_model::handle_model_entry(bus, conn, state, entry)?;
                    }
                }
                _ => {}
            }
        }
        PopupState::OpenAiAvailable {
            selected,
            filter,
            cursor,
        } => {
            let count = openai_available_entries(state, filter).len();
            match key.code {
                KeyCode::Esc => state.mode = UiMode::Popup(PopupState::ModelSelect { selected: 0 }),
                KeyCode::Up => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if *selected + 1 < count {
                        *selected += 1;
                    }
                }
                KeyCode::Enter => {
                    if state.models.openai_available_status != LoadStatus::Loaded {
                        return Ok(());
                    }
                    let filtered = openai_available_filtered(state, filter);
                    if let Some(name) = filtered.get(*selected) {
                        select_openai_model::handle_openai_available_selection(
                            bus,
                            conn,
                            state,
                            name,
                        )?;
                    }
                }
                KeyCode::Char(ch) => {
                    insert_char(filter, cursor, ch);
                    *selected = 0;
                }
                KeyCode::Backspace => {
                    delete_prev_char(filter, cursor);
                    *selected = 0;
                }
                KeyCode::Delete => {
                    delete_next_char(filter, cursor);
                    *selected = 0;
                }
                KeyCode::Left => {
                    *cursor = prev_char_boundary(filter, *cursor);
                }
                KeyCode::Right => {
                    *cursor = next_char_boundary(filter, *cursor);
                }
                KeyCode::Home => {
                    *cursor = 0;
                }
                KeyCode::End => {
                    *cursor = filter.len();
                }
                _ => {}
            }
        }
        PopupState::SettingsToggle {
            selected,
            behavior_trees,
            openai_tracing,
            web_search,
        } => match key.code {
            KeyCode::Esc => state.mode = UiMode::Normal,
            KeyCode::Up => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Down => {
                if *selected < 2 {
                    *selected += 1;
                }
            }
            KeyCode::Char(' ') => {
                let mut should_apply = true;
                match *selected {
                    0 => *behavior_trees = !*behavior_trees,
                    1 => {
                        let new_value = !*openai_tracing;
                        if new_value && insert_openai_key::api_key_missing(conn)? {
                            insert_openai_key::begin_prompt(state, true);
                            return Ok(());
                        }
                        *openai_tracing = new_value;
                    }
                    2 => {
                        if !*web_search {
                            state.popup_input = Some(PopupInput::new());
                            state.mode = UiMode::Popup(PopupState::BraveApiKeyPrompt {
                                web_search_enabled: true,
                                behavior_trees: *behavior_trees,
                                openai_tracing: *openai_tracing,
                            });
                            should_apply = false;
                        } else {
                            *web_search = false;
                        }
                    }
                    _ => {}
                }
                if should_apply {
                    change_settings::apply_settings(
                        bus,
                        conn,
                        state,
                        *behavior_trees,
                        *openai_tracing,
                        *web_search,
                    )?;
                }
            }
            KeyCode::Enter => {
                change_settings::submit_settings(
                    bus,
                    conn,
                    state,
                    *behavior_trees,
                    *openai_tracing,
                    *web_search,
                )?;
            }
            _ => {}
        },
        PopupState::BraveApiKeyPrompt {
            web_search_enabled,
            behavior_trees,
            openai_tracing,
        } => match key.code {
            KeyCode::Esc => {
                state.mode = UiMode::Normal;
                state.popup_input = None;
            }
            KeyCode::Enter => {
                let api_key = state
                    .popup_input
                    .as_ref()
                    .map(|input| input.text.clone())
                    .unwrap_or_default();
                if api_key.trim().is_empty() {
                    return Ok(());
                }
                change_settings::submit_brave_api_key(
                    bus,
                    conn,
                    state,
                    *behavior_trees,
                    *openai_tracing,
                    *web_search_enabled,
                    api_key,
                )?;
                state.popup_input = None;
            }
            _ => handle_popup_input(state, key),
        },
        PopupState::OpenAiApiKeyPrompt { enable_tracing } => match key.code {
            KeyCode::Esc => {
                state.mode = UiMode::Normal;
                state.popup_input = None;
            }
            KeyCode::Enter => {
                insert_openai_key::submit_key(bus, conn, state, *enable_tracing)?;
            }
            _ => handle_popup_input(state, key),
        },
        PopupState::ModeSelect { selected } => match key.code {
            KeyCode::Esc | KeyCode::Enter => state.mode = UiMode::Normal,
            KeyCode::Up | KeyCode::Down => *selected = 0,
            _ => {}
        },
        PopupState::PermissionPrompt { selected, .. } => {
            let options_len = permissions::option_count();
            match key.code {
                KeyCode::Esc => {
                    submit_permission_decision(bus, popup, PermissionDecision::AlwaysDeny)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Up => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if *selected + 1 < options_len {
                        *selected += 1;
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    submit_permission_decision(bus, popup, PermissionDecision::AlwaysAllow)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    submit_permission_decision(bus, popup, PermissionDecision::AlwaysDeny)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    submit_permission_decision(bus, popup, PermissionDecision::AllowOnce)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    submit_permission_decision(bus, popup, PermissionDecision::AlwaysDeny)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Enter => {
                    let decision = match *selected {
                        0 => PermissionDecision::AlwaysAllow,
                        1 => PermissionDecision::AlwaysDeny,
                        2 => PermissionDecision::AllowOnce,
                        _ => PermissionDecision::AlwaysDeny,
                    };
                    submit_permission_decision(bus, popup, decision)?;
                    state.mode = UiMode::Normal;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_popup_input(state: &mut UiState, key: KeyEvent) {
    let Some(input) = state.popup_input.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Char(ch) => insert_char(&mut input.text, &mut input.cursor, ch),
        KeyCode::Backspace => delete_prev_char(&mut input.text, &mut input.cursor),
        KeyCode::Delete => delete_next_char(&mut input.text, &mut input.cursor),
        KeyCode::Left => input.cursor = prev_char_boundary(&input.text, input.cursor),
        KeyCode::Right => input.cursor = next_char_boundary(&input.text, input.cursor),
        _ => {}
    }
}

fn submit_permission_decision(
    bus: &EventBus,
    popup: &PopupState,
    decision: PermissionDecision,
) -> Result<(), String> {
    if let PopupState::PermissionPrompt { request_id, .. } = popup {
        bus.ui_to_agent_tx
            .send(UiToAgentEvent::PermissionUpdateEvent {
                request_id: *request_id,
                decision,
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn submit_input(bus: &EventBus, conn: &Connection, state: &mut UiState) -> Result<(), String> {
    let value = input_value(state);
    let trimmed = value.trim();
    if trimmed.is_empty() {
        state.input = ratatui_textarea::TextArea::default();
        return Ok(());
    }

    match trimmed {
        "/exit" => {
            request_exit(bus, state);
        }
        ":settings" => {
            state.mode = UiMode::CommandsMenu { selected: 0 };
        }
        ":m" | ":model" | "/m" | "/model" => {
            select_openai_model::open_model_popup(conn, state);
        }
        _ => {
            let prompt = value.trim_end().to_string();
            bus.ui_to_agent_tx
                .send(UiToAgentEvent::RequestEvent { prompt })
                .map_err(|e| e.to_string())?;
        }
    }

    state.input = ratatui_textarea::TextArea::default();
    Ok(())
}

fn open_mode_popup(state: &mut UiState) {
    state.mode = UiMode::Popup(PopupState::ModeSelect { selected: 0 });
}

fn request_exit(bus: &EventBus, state: &mut UiState) {
    let _ = bus.ui_to_agent_tx.send(UiToAgentEvent::ShutdownEvent);
    state.should_quit = true;
}

fn input_is_empty(state: &UiState) -> bool {
    state.input.lines().iter().all(|line| line.is_empty())
}

fn input_value(state: &UiState) -> String {
    state.input.lines().join("\n")
}
