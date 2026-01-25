use crate::infrastructure::cli::helpers::{checkbox_item, list_state, panel_block};
use crate::infrastructure::cli::theme::{Theme, PANEL_PADDING};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::List;
use ratatui::{layout::Rect, Frame};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    behavior_trees: bool,
    openai_tracing: bool,
    web_search: bool,
    theme: Theme,
) {
    let items = vec![
        checkbox_item("Behavior trees", behavior_trees),
        checkbox_item("OpenAI tracing", openai_tracing),
        checkbox_item("Web search", web_search),
    ];
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )
        .block(panel_block(theme, theme.panel, PANEL_PADDING))
        .highlight_symbol("> ");
    let mut list_state = list_state(selected);
    frame.render_stateful_widget(list, area, &mut list_state);
}
