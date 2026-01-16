use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
/// UI utilities for the CLI
use rustyline::hint::{Hint, Hinter};
use rustyline::validate::Validator;
use rustyline::{Context, Helper};
use std::borrow::Cow;

/// ANSI escape codes for styling
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const BOX_HORIZONTAL: char = '\u{2500}';

/// Status bar helper for rustyline
/// Shows the current model and available actions below the input line
#[derive(Clone)]
pub struct StatusBarHelper {
    current_model: String,
    status_line: String,
}

impl StatusBarHelper {
    pub fn new(current_model: &str) -> Self {
        let status_line = Self::build_status_line(current_model);
        Self {
            current_model: current_model.to_string(),
            status_line,
        }
    }

    pub fn update_model(&mut self, model_name: &str) {
        self.current_model = model_name.to_string();
        self.status_line = Self::build_status_line(model_name);
    }

    fn build_status_line(model_name: &str) -> String {
        // Build the separator line (80 chars wide)
        let separator: String = std::iter::repeat(BOX_HORIZONTAL).take(80).collect();

        // Build the status content
        let status = format!(
            "current model: {}, change model (ctrl+m or :m), quit (ctrl+q or :q)",
            model_name
        );

        format!(
            "\n{}{}{}\n{}{}{}",
            DIM, separator, RESET, DIM, status, RESET
        )
    }

    pub fn current_model(&self) -> &str {
        &self.current_model
    }
}

/// Hint that contains the status bar
pub struct StatusBarHint {
    display: String,
}

impl Hint for StatusBarHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        None
    }
}

impl Hinter for StatusBarHelper {
    type Hint = StatusBarHint;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        // Always show the status bar as a hint below the input
        Some(StatusBarHint {
            display: self.status_line.clone(),
        })
    }
}

impl Highlighter for StatusBarHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        // The hint is already formatted with ANSI codes
        Cow::Borrowed(hint)
    }
}

impl Validator for StatusBarHelper {}
impl Completer for StatusBarHelper {
    type Candidate = String;
}
impl Helper for StatusBarHelper {}
