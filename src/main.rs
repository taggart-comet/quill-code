mod domain;
mod infrastructure;
mod repository;
mod utils;

use clap::Parser;
use infrastructure::{EventBus, EventController, InfrastructureInitializer};
use std::fs::OpenOptions;

#[derive(Parser)]
#[command(name = "drastis")]
#[command(about = "Local LLM inference CLI", long_about = None)]
struct Args {
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
    {
        let mut builder = env_logger::Builder::from_default_env();
        if debug {
            builder.filter_level(log::LevelFilter::Debug);
        } else {
            builder.filter_level(log::LevelFilter::Info);
        }
        builder.filter_module("rustyline", log::LevelFilter::Info);

        if let Some(project_dirs) = directories::ProjectDirs::from("com", "drastis", "drastis") {
            let log_dir = project_dirs.data_dir();
            if std::fs::create_dir_all(log_dir).is_ok() {
                let log_path = log_dir.join("drastis.log");
                if let Ok(file) = OpenOptions::new().create(true).append(true).open(log_path) {
                    builder.target(env_logger::Target::Pipe(Box::new(file)));
                }
            }
        }

        builder.init();
    }

    let infrastructure_initializer = InfrastructureInitializer::with_debug(debug);
    let infra = infrastructure_initializer.initialize()?;

    let bus = EventBus::new();
    let app_name = infra.app_name.clone();
    let controller = EventController::new(
        bus.clone(),
        infra.connection.clone(),
        infra.engine,
        app_name.clone(),
    )?;

    let cli_bus = bus.clone();
    let cli_handle = std::thread::spawn(move || infrastructure::cli::run(cli_bus, app_name));
    let controller_result = controller.run();
    let cli_result = cli_handle
        .join()
        .unwrap_or_else(|_| Err("CLI thread panicked".to_string()));

    controller_result.and(cli_result)
}
