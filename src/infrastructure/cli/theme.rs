use ratatui::style::Color;
use ratatui::widgets::block::Padding;

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub panel: Color,
    pub header_bg: Color,
    pub border: Color,
    pub info_text: Color,
    pub success: Color,
    pub error: Color,
    pub active: Color,
}

impl Theme {
    pub fn new() -> Self {
        Self {
            background: Color::Rgb(24, 27, 33),
            surface: Color::Rgb(32, 36, 44),
            panel: Color::Rgb(40, 45, 54),
            header_bg: Color::Rgb(47, 56, 64),
            border: Color::Rgb(82, 95, 105),
            info_text: Color::Rgb(210, 216, 220),
            success: Color::Rgb(104, 185, 115),
            error: Color::Rgb(208, 99, 99),
            active: Color::Rgb(245, 203, 92),
        }
    }
}

pub const PANEL_PADDING: Padding = Padding::new(2, 2, 1, 1);
pub const INPUT_PADDING: Padding = Padding::new(1, 1, 1, 1);