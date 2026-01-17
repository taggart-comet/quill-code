mod domain;
mod infrastructure;
mod repository;
mod utils;

use clap::Parser;
use domain::{CancellationToken, Session, SessionService, StartupService};
use infrastructure::{
    change_model, get_current_model_name, InfrastructureComponents, InfrastructureInitializer,
};
use rustyline::Cmd;
use rustyline::Editor;
use rustyline::KeyEvent;
use std::env;
use std::sync::Arc;
use utils::{handle_readline_error, Action, StatusBarHelper};

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
    let infra = infrastructure_initializer.initialize()?;

    println!("Application initialized. Type :q or press Ctrl+Q to exit.\n");

    // Start the REPL with infrastructure components
    repl(infra, debug)
}

fn repl(infra: InfrastructureComponents, debug: bool) -> Result<(), String> {
    // Get current model name for status bar
    let current_model_name =
        get_current_model_name(&infra.connection).unwrap_or_else(|_| "unknown".to_string());

    // Create editor with status bar helper
    let helper = StatusBarHelper::new(&current_model_name);
    let mut rl = Editor::new().map_err(|e| e.to_string())?;
    rl.set_helper(Some(helper));

    // Bind Ctrl+Q to EOF (quit)
    rl.bind_sequence(KeyEvent::ctrl('q'), Cmd::EndOfFile);

    let session_service = SessionService::new(infra.engine.clone(), infra.connection.clone())
        .map_err(|e| format!("Failed to create session service: {}", e))?;
    let startup_service =
        StartupService::with_debug(debug, infra.engine.clone(), infra.connection.clone());
    let mut current_session: Option<Session> = None;
    let mut current_engine = infra.engine.clone();

    // Set up cancellation token for Ctrl+C handling
    let cancel_token = CancellationToken::new();
    let cancel_token_handler = cancel_token.clone();

    ctrlc::set_handler(move || {
        cancel_token_handler.cancel();
    })
    .map_err(|e| format!("Failed to set Ctrl+C handler: {}", e))?;

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

                // Handle :m and :model commands to change model
                if trimmed == ":m" || trimmed == ":model" {
                    match change_model(&infra.connection.clone()) {
                        Ok(new_engine) => {
                            current_engine = new_engine;
                            // Reset session when model changes
                            current_session = None;
                            // Update the status bar with new model name
                            if let Ok(new_model_name) =
                                get_current_model_name(&infra.connection.clone())
                            {
                                if let Some(helper) = rl.helper_mut() {
                                    helper.update_model(&new_model_name);
                                }
                            }
                        }
                        Err(e) => {
                            // Check if it was just cancelled
                            let err_str = e.to_string();
                            if !err_str.contains("Cancelled") {
                                eprintln!("Error changing model: {}", e);
                            }
                        }
                    }
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                // Handle session creation or restart
                if current_session.is_none() {
                    // Create new session (without requests - session_service will create them)
                    current_session = Some(startup_service.start(trimmed)?);
                }

                let session = current_session.as_ref().unwrap();

                // Run the session service (creates request, runs workflow, updates result)
                match session_service.run(session, trimmed, &cancel_token) {
                    Ok(chain) => {
                        println!("Ok> {}", chain.get_summary());
                        println!("{}", chain.final_message().unwrap().to_string());
                    }
                    Err(domain::session::ServiceError::Workflow(
                        domain::workflow::Error::Cancelled,
                    )) => {
                        println!("\nInterrupted.");
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }

                // Reload session to get updated requests
                current_session = Some(startup_service.load_session(session.id())?);
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
                    Action::ChangeModel => {
                        // Change model
                        match change_model(&infra.connection.clone()) {
                            Ok(new_engine) => {
                                current_engine = new_engine;
                                current_session = None;
                                // Update the status bar with new model name
                                if let Ok(new_model_name) =
                                    get_current_model_name(&infra.connection.clone())
                                {
                                    if let Some(helper) = rl.helper_mut() {
                                        helper.update_model(&new_model_name);
                                    }
                                }
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                if !err_str.contains("Cancelled") {
                                    eprintln!("Error changing model: {}", e);
                                }
                            }
                        }
                        continue;
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
