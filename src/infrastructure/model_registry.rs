use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const MODELS_DIR: &str = "./models";

pub fn scan_models() -> io::Result<Vec<PathBuf>> {
    let models_path = PathBuf::from(MODELS_DIR);

    if !models_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Models directory '{}' not found", MODELS_DIR),
        ));
    }

    let mut gguf_files: Vec<PathBuf> = fs::read_dir(&models_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|ext| ext.to_string_lossy().to_lowercase() == "gguf")
                .unwrap_or(false)
        })
        .collect();

    gguf_files.sort();
    Ok(gguf_files)
}

pub fn select_model(models: Vec<PathBuf>) -> io::Result<PathBuf> {
    match models.len() {
        0 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No GGUF models found in ./models/",
        )),
        1 => {
            let model = &models[0];
            println!(
                "Auto-selected: {}",
                model.file_name().unwrap_or_default().to_string_lossy()
            );
            Ok(model.clone())
        }
        _ => prompt_user_selection(&models),
    }
}

fn prompt_user_selection(models: &[PathBuf]) -> io::Result<PathBuf> {
    println!("Available models:");
    for (i, model) in models.iter().enumerate() {
        println!(
            "  [{}] {}",
            i + 1,
            model.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    loop {
        print!("Select model (1-{}): ", models.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= models.len() => {
                return Ok(models[n - 1].clone());
            }
            _ => {
                println!(
                    "Invalid selection. Please enter a number between 1 and {}.",
                    models.len()
                );
            }
        }
    }
}
