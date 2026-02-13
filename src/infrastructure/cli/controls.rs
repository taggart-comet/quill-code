use crate::domain::permissions::UserPermissionDecision;
use crate::domain::AgentModeType;
use crate::infrastructure::cli::actions::{
    change_settings, insert_openai_key, select_openai_model,
};
use crate::infrastructure::cli::components::{commands_menu, permissions};
use crate::infrastructure::cli::helpers::{
    delete_next_char, delete_prev_char, insert_char, next_char_boundary, prev_char_boundary,
};
use crate::infrastructure::cli::state::{
    FileChangesViewMode, LoadStatus, PopupInput, PopupState, SessionPreview, TodoListViewMode,
    UiMode, UiState, MAIN_BODY_SCROLL_STEP,
};
use crate::infrastructure::cli::views::main_view::{
    model_entries, openai_available_entries, openai_available_filtered,
};
use crate::infrastructure::db::DbPool;
use crate::infrastructure::event_bus::{EventBus, UiToAgentEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

pub fn handle_key_event(
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
    key: KeyEvent,
) -> Result<(), String> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.request_in_flight.is_some() {
            let _ = bus.ui_to_agent_tx.send(UiToAgentEvent::CancelRequest);
        }
        return Ok(());
    }

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
        UiMode::FileChangesReview {
            mut selected_file,
            mut view_mode,
            mut scroll_offset,
        } => {
            handle_file_changes_key(
                state,
                key,
                &mut selected_file,
                &mut view_mode,
                &mut scroll_offset,
            )?;
            // Update state with new values if still in review mode
            if matches!(state.mode, UiMode::FileChangesReview { .. }) {
                state.mode = UiMode::FileChangesReview {
                    selected_file,
                    view_mode,
                    scroll_offset,
                };
            }
            Ok(())
        }
        UiMode::TodoListReview {
            mut selected_item,
            mut view_mode,
            mut scroll_offset,
        } => {
            handle_todo_list_key(
                state,
                key,
                &mut selected_item,
                &mut view_mode,
                &mut scroll_offset,
            )?;
            // Update state with new values if still in review mode
            if matches!(state.mode, UiMode::TodoListReview { .. }) {
                state.mode = UiMode::TodoListReview {
                    selected_item,
                    view_mode,
                    scroll_offset,
                };
            }
            Ok(())
        }
    }
}

pub fn handle_mouse_event(state: &mut UiState, mouse: MouseEvent) {
    // Allow scrolling in FileChangesReview diff view
    if let UiMode::FileChangesReview {
        view_mode: FileChangesViewMode::UnifiedDiff,
        scroll_offset,
        ..
    } = &mut state.mode
    {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                *scroll_offset = scroll_offset.saturating_sub(MAIN_BODY_SCROLL_STEP);
            }
            MouseEventKind::ScrollDown => {
                *scroll_offset = scroll_offset.saturating_add(MAIN_BODY_SCROLL_STEP);
            }
            _ => {}
        }
        return;
    }

    // Allow scrolling in TodoListReview detail view
    if let UiMode::TodoListReview {
        view_mode: TodoListViewMode::ItemDetail,
        scroll_offset,
        ..
    } = &mut state.mode
    {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                *scroll_offset = scroll_offset.saturating_sub(MAIN_BODY_SCROLL_STEP);
            }
            MouseEventKind::ScrollDown => {
                *scroll_offset = scroll_offset.saturating_add(MAIN_BODY_SCROLL_STEP);
            }
            _ => {}
        }
        return;
    }

    if !matches!(state.mode, UiMode::Normal) {
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            scroll_up(state, MAIN_BODY_SCROLL_STEP);
        }
        MouseEventKind::ScrollDown => {
            scroll_down(state, MAIN_BODY_SCROLL_STEP);
        }
        _ => {}
    }
}

fn scroll_up(state: &mut UiState, amount: usize) {
    state.main_body_scroll = state.main_body_scroll.saturating_add(amount);
    state.main_body_follow = false;
}

fn scroll_down(state: &mut UiState, amount: usize) {
    state.main_body_scroll = state.main_body_scroll.saturating_sub(amount);
    if state.main_body_scroll == 0 {
        state.main_body_follow = true;
    }
}

fn handle_normal_key(
    bus: &EventBus,
    conn: &DbPool,
    state: &mut UiState,
    key: KeyEvent,
) -> Result<(), String> {
    match key.code {
        KeyCode::Up
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            scroll_up(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::Down
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            scroll_down(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_up(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_down(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::PageUp => {
            scroll_up(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::PageDown => {
            scroll_down(state, MAIN_BODY_SCROLL_STEP);
            return Ok(());
        }
        KeyCode::Char('v')
            if key.modifiers.contains(KeyModifiers::SUPER)
                || key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            // Cmd+V on macOS or Ctrl+V on others
            log::info!("🔑 Cmd+V key detected, attempting to handle paste");
            if let Err(e) = handle_paste(state) {
                log::warn!("❌ Paste failed: {}", e);
                state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
                    text: format!("⚠️ Paste failed: {}", e),
                    kind: crate::infrastructure::cli::state::ProgressKind::Error,
                    active: false,
                    request_id: None,
                });
            }
        }
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
            // Enter right_container review mode for whatever is displayed
            let has_file_changes = state.file_changes.is_some()
                && !state.file_changes.as_ref().unwrap().changes.is_empty();
            let has_todo_list =
                state.todo_list.is_some() && !state.todo_list.as_ref().unwrap().items.is_empty();

            // Priority: file changes first (if both exist), otherwise whichever exists
            if has_file_changes {
                state.mode = UiMode::FileChangesReview {
                    selected_file: 0,
                    view_mode: FileChangesViewMode::FileList,
                    scroll_offset: 0,
                };
            } else if has_todo_list {
                state.mode = UiMode::TodoListReview {
                    selected_item: 0,
                    view_mode: TodoListViewMode::ItemList,
                    scroll_offset: 0,
                };
            }
            return Ok(());
        }
        KeyCode::BackTab => {
            // BackTab = Shift+Tab on most terminals
            // Enter right_container review mode for whatever is displayed
            let has_file_changes = state.file_changes.is_some()
                && !state.file_changes.as_ref().unwrap().changes.is_empty();
            let has_todo_list =
                state.todo_list.is_some() && !state.todo_list.as_ref().unwrap().items.is_empty();

            // Priority: file changes first (if both exist), otherwise whichever exists
            if has_file_changes {
                state.mode = UiMode::FileChangesReview {
                    selected_file: 0,
                    view_mode: FileChangesViewMode::FileList,
                    scroll_offset: 0,
                };
            } else if has_todo_list {
                state.mode = UiMode::TodoListReview {
                    selected_item: 0,
                    view_mode: TodoListViewMode::ItemList,
                    scroll_offset: 0,
                };
            }
            return Ok(());
        }
        KeyCode::Tab if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            // Toggle agent mode. Plan with a pending TODO list → BuildFromPlan;
            // BuildFromPlan escapes back to Build.
            state.agent_mode = match state.agent_mode {
                AgentModeType::Build => AgentModeType::Plan,
                AgentModeType::Plan => {
                    let has_pending = state
                        .todo_list
                        .as_ref()
                        .map(|list| {
                            !list.items.is_empty()
                                && list.items.iter().any(|item| item.status != "completed")
                        })
                        .unwrap_or(false);
                    if has_pending {
                        AgentModeType::BuildFromPlan
                    } else {
                        AgentModeType::Build
                    }
                }
                AgentModeType::BuildFromPlan => AgentModeType::Build,
            };
        }
        KeyCode::Esc if !state.attached_images.is_empty() => {
            // Remove all attached images when Esc is pressed
            state.attached_images.clear();
        }
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
    conn: &DbPool,
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
            3 => open_continue_popup(conn, state),
            4 => request_exit(bus, state),
            _ => state.mode = UiMode::Normal,
        },
        _ => {}
    }

    Ok(())
}

fn handle_popup_key(
    bus: &EventBus,
    conn: &DbPool,
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
                            bus, conn, state, name,
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
            max_tool_calls,
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
                                max_tool_calls: *max_tool_calls,
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
                        *max_tool_calls,
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
                    *max_tool_calls,
                )?;
            }
            _ => {}
        },
        PopupState::BraveApiKeyPrompt {
            web_search_enabled,
            behavior_trees,
            openai_tracing,
            max_tool_calls,
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
                    *max_tool_calls,
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
            KeyCode::Esc => state.mode = UiMode::Normal,
            KeyCode::Up => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Down => {
                if *selected < 1 {
                    *selected += 1;
                }
            }
            KeyCode::Enter => {
                state.agent_mode = match *selected {
                    1 => AgentModeType::Plan,
                    _ => AgentModeType::Build,
                };
                state.mode = UiMode::Normal;
            }
            _ => {}
        },
        PopupState::PermissionPrompt {
            selected,
            is_read_only,
            command,
            ..
        } => {
            let options_len = permissions::option_count();
            let has_command = command.is_some();
            match key.code {
                KeyCode::Esc | KeyCode::Char('d') | KeyCode::Char('D') => {
                    submit_permission_decision(bus, popup, UserPermissionDecision::Deny)?;
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
                    submit_permission_decision(bus, popup, UserPermissionDecision::AllowOnce)?;
                    state.mode = UiMode::Normal;
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if has_command {
                        submit_permission_decision(
                            bus,
                            popup,
                            UserPermissionDecision::AlwaysAllow,
                        )?;
                        state.mode = UiMode::Normal;
                    }
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if !has_command && *is_read_only {
                        submit_permission_decision(
                            bus,
                            popup,
                            UserPermissionDecision::AlwaysAllow,
                        )?;
                        state.mode = UiMode::Normal;
                    }
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    if !has_command && !*is_read_only {
                        submit_permission_decision(
                            bus,
                            popup,
                            UserPermissionDecision::AlwaysAllow,
                        )?;
                        state.mode = UiMode::Normal;
                    }
                }
                KeyCode::Enter => {
                    let decision = match *selected {
                        0 => UserPermissionDecision::AllowOnce,
                        1 => UserPermissionDecision::AlwaysAllow,
                        2 => UserPermissionDecision::Deny,
                        _ => UserPermissionDecision::Deny,
                    };
                    submit_permission_decision(bus, popup, decision)?;
                    state.mode = UiMode::Normal;
                }
                _ => {}
            }
        }
        PopupState::AuthMethodSelect { selected } => match key.code {
            KeyCode::Esc => state.mode = UiMode::Popup(PopupState::ModelSelect { selected: 0 }),
            KeyCode::Up => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Down => {
                if *selected < 1 {
                    *selected += 1;
                }
            }
            KeyCode::Enter => match *selected {
                0 => {
                    select_openai_model::handle_auth_method_api_key(conn, state)?;
                }
                1 => {
                    select_openai_model::handle_auth_method_oauth(bus, conn, state)?;
                }
                _ => {}
            },
            _ => {}
        },
        PopupState::ContinueSelect { sessions, selected } => {
            let count = sessions.len();
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
                    let session_id = sessions[*selected].id;
                    state.session_id = Some(session_id);
                    let _ = bus
                        .ui_to_agent_tx
                        .send(UiToAgentEvent::SessionContinueEvent { session_id });
                    state.mode = UiMode::Normal;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_file_changes_key(
    state: &mut UiState,
    key: KeyEvent,
    selected_file: &mut usize,
    view_mode: &mut FileChangesViewMode,
    scroll_offset: &mut usize,
) -> Result<(), String> {
    match view_mode {
        FileChangesViewMode::FileList => {
            let file_count = state
                .file_changes
                .as_ref()
                .map(|fc| fc.changes.len())
                .unwrap_or(0);

            match key.code {
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::BackTab => {
                    // BackTab = Shift+Tab on most terminals (return to Normal)
                    state.mode = UiMode::Normal;
                }
                KeyCode::Esc => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::Left => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::Up => {
                    if *selected_file > 0 {
                        *selected_file -= 1;
                    }
                }
                KeyCode::Down => {
                    if *selected_file + 1 < file_count {
                        *selected_file += 1;
                    }
                }
                KeyCode::Enter => {
                    // Switch to UnifiedDiff view
                    *view_mode = FileChangesViewMode::UnifiedDiff;
                    *scroll_offset = 0;
                }
                _ => {}
            }
        }
        FileChangesViewMode::UnifiedDiff => {
            // Get the current diff line count to prevent over-scrolling
            let diff_line_count = state
                .file_changes
                .as_ref()
                .and_then(|fc| fc.changes.get(*selected_file))
                .map(|change| change.unified_diff.lines().count())
                .unwrap_or(0);

            match key.code {
                KeyCode::Esc | KeyCode::Left => {
                    // Return to FileList view
                    *view_mode = FileChangesViewMode::FileList;
                    *scroll_offset = 0;
                }
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::BackTab => {
                    // BackTab = Shift+Tab on most terminals (return to Normal)
                    state.mode = UiMode::Normal;
                }
                KeyCode::Up => {
                    *scroll_offset = scroll_offset.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *scroll_offset < diff_line_count.saturating_sub(1) {
                        *scroll_offset += 1;
                    }
                }
                KeyCode::PageUp => {
                    *scroll_offset = scroll_offset.saturating_sub(MAIN_BODY_SCROLL_STEP);
                }
                KeyCode::PageDown => {
                    let max_scroll = diff_line_count.saturating_sub(1);
                    *scroll_offset = (*scroll_offset + MAIN_BODY_SCROLL_STEP).min(max_scroll);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_todo_list_key(
    state: &mut UiState,
    key: KeyEvent,
    selected_item: &mut usize,
    view_mode: &mut TodoListViewMode,
    scroll_offset: &mut usize,
) -> Result<(), String> {
    match view_mode {
        TodoListViewMode::ItemList => {
            let item_count = state
                .todo_list
                .as_ref()
                .map(|tl| tl.items.len())
                .unwrap_or(0);

            match key.code {
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::BackTab => {
                    // BackTab = Shift+Tab on most terminals (return to Normal)
                    state.mode = UiMode::Normal;
                }
                KeyCode::Esc => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::Up => {
                    if *selected_item > 0 {
                        *selected_item -= 1;
                    }
                }
                KeyCode::Down => {
                    if *selected_item + 1 < item_count {
                        *selected_item += 1;
                    }
                }
                KeyCode::Enter => {
                    // Switch to ItemDetail view
                    *view_mode = TodoListViewMode::ItemDetail;
                    *scroll_offset = 0;
                }
                _ => {}
            }
        }
        TodoListViewMode::ItemDetail => {
            // Get the current description line count to prevent over-scrolling
            let description_line_count = state
                .todo_list
                .as_ref()
                .and_then(|tl| tl.items.get(*selected_item))
                .map(|item| item.description.lines().count())
                .unwrap_or(0);

            match key.code {
                KeyCode::Esc | KeyCode::Backspace => {
                    // Return to ItemList view
                    *view_mode = TodoListViewMode::ItemList;
                    *scroll_offset = 0;
                }
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Return to Normal mode
                    state.mode = UiMode::Normal;
                }
                KeyCode::BackTab => {
                    // BackTab = Shift+Tab on most terminals (return to Normal)
                    state.mode = UiMode::Normal;
                }
                KeyCode::Up => {
                    *scroll_offset = scroll_offset.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *scroll_offset < description_line_count.saturating_sub(1) {
                        *scroll_offset += 1;
                    }
                }
                KeyCode::PageUp => {
                    *scroll_offset = scroll_offset.saturating_sub(MAIN_BODY_SCROLL_STEP);
                }
                KeyCode::PageDown => {
                    let max_scroll = description_line_count.saturating_sub(1);
                    *scroll_offset = (*scroll_offset + MAIN_BODY_SCROLL_STEP).min(max_scroll);
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
    decision: UserPermissionDecision,
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

const MAX_IMAGE_SIZE: usize = 20 * 1024 * 1024; // 20MB OpenAI limit
const MAX_IMAGES_PER_REQUEST: usize = 10;

pub fn handle_paste_event(state: &mut UiState, pasted_text: String) -> Result<(), String> {
    log::info!("========== PASTE EVENT RECEIVED ==========");
    log::info!("Pasted text length: {} chars", pasted_text.len());

    if pasted_text.is_empty() {
        log::info!("⚠️ EMPTY paste text - likely an image paste from screenshot");
    } else {
        log::info!(
            "First 100 chars: {:?}",
            pasted_text.chars().take(100).collect::<String>()
        );
    }

    // Check if model supports images
    let can_attach = state.can_attach_images();
    log::info!(
        "Can attach images: {}, current model: {}",
        can_attach,
        state.current_model
    );

    if !can_attach {
        log::info!("Model doesn't support images, inserting as text");
        for ch in pasted_text.chars() {
            state.input.insert_char(ch);
        }
        return Ok(());
    }

    // Check if we've hit the image limit
    if state.attached_images.len() >= MAX_IMAGES_PER_REQUEST {
        log::warn!(
            "Maximum {} images per request, inserting as text",
            MAX_IMAGES_PER_REQUEST
        );
        for ch in pasted_text.chars() {
            state.input.insert_char(ch);
        }
        return Ok(());
    }

    // STEP 1: Try to get file paths from clipboard (when files copied in Finder)
    log::info!("=== Starting clipboard check ===");
    log::info!("Creating clipboard reader...");
    let mut clipboard = match crate::infrastructure::cli::ClipboardReader::new() {
        Ok(cb) => {
            log::info!("✅ Clipboard reader created successfully");
            cb
        }
        Err(e) => {
            log::error!("❌ Failed to create clipboard reader: {:?}", e);
            log::info!("Falling back to file path check");
            return handle_possible_file_path(state, pasted_text);
        }
    };

    // Try to read file list FIRST (Finder file copy)
    log::info!("Attempting to read file list from clipboard...");
    match clipboard.get_files() {
        Ok(files) if !files.is_empty() => {
            log::info!("✅ Found {} file(s) in clipboard", files.len());

            // Try to load the first file if it's an image
            if let Some(first_file) = files.first() {
                if let Some(ext) = first_file.extension() {
                    let ext_lower = ext.to_string_lossy().to_lowercase();
                    if matches!(
                        ext_lower.as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
                    ) {
                        log::info!("First file is an image, loading: {:?}", first_file);
                        return load_image_from_file(state, first_file);
                    }
                }
            }

            // Not an image, insert file path as text
            log::info!("File is not an image, inserting path as text");
            if let Some(first_file) = files.first() {
                let path_str = first_file.to_string_lossy();
                for ch in path_str.chars() {
                    state.input.insert_char(ch);
                }
            }
            return Ok(());
        }
        Ok(_) => {
            log::info!("No files in clipboard, trying image data");
        }
        Err(e) => {
            log::warn!("Failed to read file list: {:?}, trying image data", e);
        }
    }

    // STEP 2: Try to read image data from clipboard (for screenshots)
    log::info!("Attempting to read image from clipboard...");
    match clipboard.read_image() {
        Ok((base64_data, mime_type, size)) => {
            log::info!(
                "✅✅✅ SUCCESS! Image found in clipboard: {} bytes, type: {}",
                size,
                mime_type
            );

            if size > MAX_IMAGE_SIZE {
                log::warn!(
                    "⚠️ Image too large: {} bytes (max: {} bytes), trying file path instead",
                    size,
                    MAX_IMAGE_SIZE
                );
                return handle_possible_file_path(state, pasted_text);
            }

            let formatted_size = crate::infrastructure::cli::format_size(size);
            log::info!("Creating AttachedImage with size: {}", formatted_size);
            let image = crate::infrastructure::cli::state::AttachedImage::new(
                base64_data,
                mime_type,
                formatted_size,
            );
            state.attached_images.push(image);
            log::info!(
                "🎉🎉🎉 Image attached from clipboard successfully! Total images: {}",
                state.attached_images.len()
            );

            Ok(())
        }
        Err(crate::infrastructure::cli::ClipboardError::NotAnImage) => {
            log::info!("❌ No image in clipboard (NotAnImage error)");
            log::info!("Checking if pasted text is a file path");
            handle_possible_file_path(state, pasted_text)
        }
        Err(e) => {
            log::error!("❌ Clipboard read error: {:?}", e);
            log::info!("Falling back to file path check");
            handle_possible_file_path(state, pasted_text)
        }
    }
}

fn handle_possible_file_path(state: &mut UiState, pasted_text: String) -> Result<(), String> {
    let trimmed = pasted_text.trim();
    log::info!("handle_possible_file_path: trimmed text = '{}'", trimmed);

    if trimmed.is_empty() {
        log::info!("Empty paste, doing nothing");
        return Ok(());
    }

    // Try to parse as a file path, check common locations if just a filename
    let path = std::path::Path::new(trimmed);

    // If it's just a filename (no path separator), search in common locations
    let possible_paths = if !trimmed.contains('/') && !trimmed.contains('\\') {
        log::info!(
            "No path separator found, checking common locations for: {}",
            trimmed
        );
        let mut paths = vec![Some(std::path::PathBuf::from(trimmed))]; // Current directory

        if let Ok(home) = std::env::var("HOME") {
            let home_path = std::path::PathBuf::from(home);
            paths.push(Some(home_path.join("Desktop").join(trimmed)));
            paths.push(Some(home_path.join("Downloads").join(trimmed)));
        }

        paths
    } else {
        vec![Some(path.to_path_buf())]
    };

    // Try each possible path
    for possible_path in possible_paths.iter().flatten() {
        log::info!("Checking path: {:?}", possible_path);

        if possible_path.exists() && possible_path.is_file() {
            log::info!("✅ Found file at: {:?}", possible_path);

            // Check if it's an image
            if let Some(ext) = possible_path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                log::info!("File extension: {}", ext_lower);

                if matches!(
                    ext_lower.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
                ) {
                    log::info!("Extension matches image format, loading file");
                    return load_image_from_file(state, possible_path);
                } else {
                    log::warn!("Extension '{}' is not an image format", ext_lower);
                }
            }
        }
    }

    log::info!("File not found in any location");

    // Not a file path or not an image, insert as text
    log::info!(
        "Not a valid image file path, inserting '{}' as text ({} chars)",
        trimmed,
        trimmed.len()
    );
    for ch in pasted_text.chars() {
        state.input.insert_char(ch);
    }
    Ok(())
}

fn load_image_from_file(state: &mut UiState, path: &std::path::Path) -> Result<(), String> {
    use base64::Engine;

    log::info!("=== Loading image from file ===");
    log::info!("File path: {:?}", path);

    // Read the file
    let file_data = std::fs::read(path).map_err(|e| format!("Failed to read image file: {}", e))?;

    let size = file_data.len();
    log::info!("File size: {} bytes", size);

    if size > MAX_IMAGE_SIZE {
        log::warn!("⚠️ Image file too large: {} bytes", size);
        let formatted_size = crate::infrastructure::cli::format_size(size);
        state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
            text: format!("⚠️ Image too large: {} (max 20MB)", formatted_size),
            kind: crate::infrastructure::cli::state::ProgressKind::Error,
            active: false,
            request_id: None,
        });
        return Err(format!("Image too large: {} (max 20MB)", formatted_size));
    }

    // Determine MIME type from extension
    let mime_type = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/png", // default
    };

    // Base64 encode
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&file_data);
    let formatted_size = crate::infrastructure::cli::format_size(size);

    let image = crate::infrastructure::cli::state::AttachedImage::new(
        base64_data,
        mime_type.to_string(),
        formatted_size.clone(),
    );

    state.attached_images.push(image);
    log::info!(
        "✅ Image attached successfully! Total: {}",
        state.attached_images.len()
    );

    // Show success message
    state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
        text: format!("✅ Image attached ({})", formatted_size),
        kind: crate::infrastructure::cli::state::ProgressKind::Success,
        active: false,
        request_id: None,
    });

    Ok(())
}

fn handle_paste(state: &mut UiState) -> Result<(), String> {
    log::info!("handle_paste called");

    // Check if model supports images
    let can_attach = state.can_attach_images();
    log::info!(
        "Can attach images: {}, current model: {}",
        can_attach,
        state.current_model
    );

    if !can_attach {
        log::warn!("Image pasting requires an OpenAI model, falling back to text");
        return paste_text(state);
    }

    // Check if we've hit the image limit
    if state.attached_images.len() >= MAX_IMAGES_PER_REQUEST {
        log::warn!("Maximum {} images per request", MAX_IMAGES_PER_REQUEST);
        return Ok(());
    }

    // Try to get clipboard reader
    log::info!("Creating clipboard reader");
    let mut clipboard = match crate::infrastructure::cli::ClipboardReader::new() {
        Ok(cb) => {
            log::info!("Clipboard reader created successfully");
            cb
        }
        Err(e) => {
            log::warn!(
                "Failed to create clipboard reader: {:?}, falling back to text",
                e
            );
            return paste_text(state);
        }
    };

    // Try to read image
    log::info!("Attempting to read image from clipboard");
    match clipboard.read_image() {
        Ok((base64_data, mime_type, size)) => {
            log::info!(
                "Image read successfully: {} bytes, type: {}",
                size,
                mime_type
            );

            if size > MAX_IMAGE_SIZE {
                log::warn!(
                    "Image too large: {} bytes (max: {} bytes)",
                    size,
                    MAX_IMAGE_SIZE
                );
                return Ok(());
            }

            let formatted_size = crate::infrastructure::cli::format_size(size);
            let image = crate::infrastructure::cli::state::AttachedImage::new(
                base64_data,
                mime_type,
                formatted_size.clone(),
            );
            state.attached_images.push(image);
            log::info!(
                "Image attached successfully, total images: {}",
                state.attached_images.len()
            );

            // Show success message to user
            state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
                text: format!("✅ Image attached ({})", formatted_size),
                kind: crate::infrastructure::cli::state::ProgressKind::Success,
                active: false,
                request_id: None,
            });

            Ok(())
        }
        Err(crate::infrastructure::cli::ClipboardError::NotAnImage) => {
            log::info!("Clipboard does not contain an image, trying text paste");
            paste_text(state)
        }
        Err(e) => {
            log::warn!("Clipboard read error: {:?}, trying text paste", e);
            paste_text(state)
        }
    }
}

fn paste_text(state: &mut UiState) -> Result<(), String> {
    log::info!("paste_text called");

    // Try to paste as text into the TextArea
    let mut clipboard = match crate::infrastructure::cli::ClipboardReader::new() {
        Ok(cb) => {
            log::info!("Clipboard reader created for text paste");
            cb
        }
        Err(e) => {
            log::error!("Clipboard not available for text paste: {:?}", e);
            return Err(format!("Clipboard not available: {:?}", e));
        }
    };

    if let Ok(text) = clipboard.get_text() {
        log::info!("Text read from clipboard: {} chars", text.len());
        // Insert text at cursor position
        for ch in text.chars() {
            state.input.insert_char(ch);
        }
        log::info!("Text pasted successfully");
    } else {
        log::warn!("Failed to read text from clipboard");
    }

    Ok(())
}

fn submit_input(bus: &EventBus, conn: &DbPool, state: &mut UiState) -> Result<(), String> {
    let value = input_value(state);
    let trimmed = value.trim();
    if trimmed.is_empty() {
        state.clear_input_and_attachments();
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
        "/continue" | "/c" => {
            open_continue_popup(conn, state);
        }
        _ => {
            let prompt = value.trim_end().to_string();

            // Convert attached images to event format
            let images = state
                .attached_images
                .iter()
                .map(|img| crate::infrastructure::event_bus::ImageAttachment {
                    data_url: img.to_data_url(),
                })
                .collect();

            bus.ui_to_agent_tx
                .send(UiToAgentEvent::RequestEvent {
                    prompt,
                    images,
                    mode: state.agent_mode,
                    session_id: state.session_id,
                })
                .map_err(|e| e.to_string())?;
        }
    }

    state.clear_input_and_attachments();
    Ok(())
}

fn open_continue_popup(conn: &DbPool, state: &mut UiState) {
    let result = (|| -> Result<(), String> {
        let conn_guard = conn
            .get()
            .map_err(|e| format!("Failed to get connection: {}", e))?;

        let project_name = std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".to_string());

        let projects_repo = crate::repository::ProjectsRepository::new(&*conn_guard);
        let project = projects_repo
            .find_by_name(&project_name)?
            .ok_or_else(|| "not_found".to_string())?;

        let sessions_repo = crate::repository::SessionsRepository::new(&*conn_guard);
        let rows = sessions_repo.find_by_project_recent(project.id, 5)?;

        if rows.is_empty() {
            return Err("no_sessions".to_string());
        }

        let sessions: Vec<SessionPreview> = rows
            .into_iter()
            .map(|row| {
                let epoch: u64 = row.created_at.parse().unwrap_or(0);
                SessionPreview {
                    id: row.id,
                    name: row.name,
                    created_at: crate::infrastructure::cli::state::format_relative_time(epoch),
                }
            })
            .collect();

        state.mode = UiMode::Popup(PopupState::ContinueSelect {
            sessions,
            selected: 0,
        });
        Ok(())
    })();

    match result {
        Ok(()) => {}
        Err(e) if e == "not_found" => {
            state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
                text: "No project found for current directory".to_string(),
                kind: crate::infrastructure::cli::state::ProgressKind::Error,
                active: false,
                request_id: None,
            });
        }
        Err(e) if e == "no_sessions" => {
            state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
                text: "No previous sessions in this project".to_string(),
                kind: crate::infrastructure::cli::state::ProgressKind::Info,
                active: false,
                request_id: None,
            });
        }
        Err(e) => {
            state.push_progress(crate::infrastructure::cli::state::ProgressEntry {
                text: format!("Error loading sessions: {}", e),
                kind: crate::infrastructure::cli::state::ProgressKind::Error,
                active: false,
                request_id: None,
            });
        }
    }
}

fn open_mode_popup(state: &mut UiState) {
    let selected = match state.agent_mode {
        AgentModeType::Build => 0,
        AgentModeType::Plan => 1,
        AgentModeType::BuildFromPlan => 0,
    };
    state.mode = UiMode::Popup(PopupState::ModeSelect { selected });
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
