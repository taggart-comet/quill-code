mod utils;
mod domain;
mod infrastructure;
mod repository;

use clap::Parser;
use domain::{StartupService, SessionService, Session, CancellationToken};
use infrastructure::InfrastructureInitializer;
use rustyline::DefaultEditor;
use rustyline::KeyEvent;
use rustyline::Cmd;
use utils::{handle_readline_error, Action};
use std::env;

#[derive(Parser)]
#[command(name = "drastis")]
#[command(about = "Local LLM inference CLI", long_about = None)]
struct Args {
    /// Enable debug output from llama.cpp
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args.debug) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(debug: bool) -> Result<(), String> {
    // Initialize logging
    {
        let mut builder = env_logger::Builder::from_default_env();

        // Our crate logs: respect debug flag
        if debug {
            builder.filter_level(log::LevelFilter::Debug);
        } else {
            builder.filter_level(log::LevelFilter::Info);
        }

        // Always silence very chatty deps like rustyline below DEBUG
        builder.filter_module("rustyline", log::LevelFilter::Info);

        builder.init();
    }

    // Initialize infrastructure components
    let infrastructure_initializer = InfrastructureInitializer::with_debug(debug);
    let infrastructure = infrastructure_initializer.initialize()?;
    
    println!("Application initialized. Type :q or press Ctrl+Q to exit.\n");

    // Start the REPL with infrastructure components
    repl(infrastructure.connection, infrastructure.engine, debug)
}


fn repl(
    connection: rusqlite::Connection,
    engine: infrastructure::InferenceEngine,
    debug: bool,
) -> Result<(), String> {
    let mut rl = DefaultEditor::new().map_err(|e| e.to_string())?;

    // Bind Ctrl+Q to EOF (quit)
    rl.bind_sequence(KeyEvent::ctrl('q'), Cmd::EndOfFile);

    let session_service = SessionService::new();
    let startup_service = StartupService::with_debug(debug);
    let mut current_session: Option<Session> = None;

    // Set up cancellation token for Ctrl+C handling
    let cancel_token = CancellationToken::new();
    let cancel_token_handler = cancel_token.clone();

    ctrlc::set_handler(move || {
        cancel_token_handler.cancel();
    }).map_err(|e| format!("Failed to set Ctrl+C handler: {}", e))?;

    // Load history if exists
    let history_path = dirs_home().join(".drastis_history");
    let _ = rl.load_history(&history_path);

    loop {
        // Reset cancellation token before each prompt
        cancel_token.reset();

        match rl.readline("You> ") {
            Ok(line) => {
                let trimmed = line.trim();

                if trimmed.is_empty() {
                    continue;
                }

                // Handle :q and :quit commands
                if trimmed == ":q" || trimmed == ":quit" {
                    println!("Goodbye!");
                    break;
                }

                let _ = rl.add_history_entry(&line);

                // Handle session creation or restart
                if current_session.is_none() {
                    // Create new session (without requests - session_service will create them)
                    current_session = Some(startup_service.start(&connection, &engine, trimmed)?);
                }

                let session = current_session.as_ref().unwrap();

                // Run the session service (creates request, runs workflow, updates result)
                match session_service.run(session, trimmed, &engine, &cancel_token, &connection) {
                    Ok(chain) => {
                        let finish_message = chain.get_finish_message();
                        println!("Ok> {}", finish_message);
                    }
                    Err(domain::session::ServiceError::Workflow(domain::workflow::Error::Cancelled)) => {
                        println!("\nInterrupted.");
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }

                // Reload session to get updated requests
                current_session = Some(startup_service.load_session(&connection, session.id())?);
            }
            Err(err) => {
                // Handle keyboard shortcuts (Ctrl+C, Ctrl+Q, Ctrl+D)
                match handle_readline_error(err)? {
                    Action::Cancel => {
                        // Ctrl+C - cancel current operation
                        println!("^C");
                        continue;
                    }
                    Action::Quit => {
                        // Ctrl+Q or Ctrl+D - quit the application
                        println!("Goodbye!");
                        break;
                    }
                    Action::Continue => {
                        // Should not happen, but continue if it does
                        continue;
                    }
                }
            }
        }
    }

    // Save history
    let _ = rl.save_history(&history_path);

    Ok(())
}


fn dirs_home() -> std::path::PathBuf {
    env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

