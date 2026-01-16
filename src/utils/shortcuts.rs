use rustyline::error::ReadlineError;

/// Handle readline errors, including shortcut-triggered exits
/// Ctrl+C triggers Interrupted, Ctrl+D and Ctrl+Q trigger EOF
pub fn handle_readline_error(err: ReadlineError) -> Result<Action, String> {
    match err {
        ReadlineError::Interrupted => {
            // Ctrl+C - cancel current operation
            Ok(Action::Cancel)
        }
        ReadlineError::Eof => {
            // Ctrl+D or Ctrl+Q - quit the application
            Ok(Action::Quit)
        }
        err => Err(format!("Readline error: {:?}", err)),
    }
}

/// Actions that can be triggered by keyboard shortcuts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Continue normal operation
    Continue,
    /// Cancel current operation (Ctrl+C)
    Cancel,
    /// Quit the application (Ctrl+Q, Ctrl+D)
    Quit,
    /// Change model (Ctrl+M)
    ChangeModel,
}
