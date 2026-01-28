use crate::domain::AgentModeType;
use crate::infrastructure::cli::helpers::panel_block;
use crate::infrastructure::cli::state::{FileChangesViewMode, TodoListViewMode, UiMode, UiState};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: Theme) {
    let (mode_name, mode_color) = match state.agent_mode {
        AgentModeType::Build => ("BUILD", theme.info_text), // Blue
        AgentModeType::Plan => ("PLAN", Color::LightGreen),
    };

    // Determine hint based on current mode
    let hint = match &state.mode {
        UiMode::FileChangesReview { view_mode, .. } => match view_mode {
            FileChangesViewMode::FileList => {
                "↑/↓ Select | Enter to View Diff | Shift+Tab to Return"
            }
            FileChangesViewMode::UnifiedDiff => {
                "↑/↓ Scroll | Esc to File List | Shift+Tab to Return"
            }
        },
        UiMode::TodoListReview { view_mode, .. } => match view_mode {
            TodoListViewMode::ItemList => "↑/↓ Select | Enter to View Detail | Shift+Tab to Return",
            TodoListViewMode::ItemDetail => "↑/↓ Scroll | Esc to Item List | Shift+Tab to Return",
        },
        UiMode::Normal => {
            let has_file_changes = state.file_changes.is_some()
                && !state.file_changes.as_ref().unwrap().changes.is_empty();
            let has_todo_list =
                state.todo_list.is_some() && !state.todo_list.as_ref().unwrap().items.is_empty();

            let has_left_panel = has_file_changes || has_todo_list;
            if has_left_panel {
                "Shift+Tab to Review Left Panel | / for Menu"
            } else {
                "/ for Menu"
            }
        }
        UiMode::CommandsMenu { .. } => "↑/↓ Select | Enter to Choose | Esc to Cancel",
        UiMode::Popup(_) => "Follow popup instructions",
    };

    let (primary_hint, secondary_hint) = hint
        .split_once(" | ")
        .map(|(primary, secondary)| (primary, Some(secondary)))
        .unwrap_or((hint, None));
    let hint_style = Style::default()
        .fg(theme.info_text)
        .add_modifier(Modifier::DIM);

    let mut line_spans = vec![
        Span::styled(
            format!(" {} ", mode_name),
            Style::default()
                .fg(theme.background)
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Model:", Style::default().fg(theme.info_text)),
        Span::raw(" "),
        Span::styled(
            state.current_model.clone(),
            Style::default()
                .fg(theme.info_text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("| ", hint_style),
        Span::styled(primary_hint, hint_style),
    ];
    if let Some(secondary_hint) = secondary_hint {
        line_spans.push(Span::styled(" | ", hint_style));
        line_spans.push(Span::styled(secondary_hint, hint_style));
    }
    let line = Line::from(line_spans);

    let panel = Paragraph::new(line).block(panel_block(theme, theme.surface, PANEL_PADDING));
    frame.render_widget(panel, area);
}