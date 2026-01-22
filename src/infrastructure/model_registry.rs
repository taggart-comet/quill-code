use std::fs;
use std::io;
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
