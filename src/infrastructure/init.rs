use crate::domain::ModelType;
use crate::infrastructure::db;
use crate::infrastructure::inference::openai::OpenAIEngine;
use crate::infrastructure::inference::{local::LocalEngine, InferenceEngine};
use crate::infrastructure::model_registry;
use crate::repository::{MetaRepository, ModelsRepository};
use rusqlite::Connection;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Error type for infrastructure initialization
#[derive(Debug, Error)]
pub enum InitError {
    #[error("database error: {0}")]
    Database(String),

    #[error("model not found: id {0}")]
    ModelNotFound(i64),

    #[error("local model missing gguf_file_path")]
    MissingGgufPath,

    #[error("model file not found: {0}")]
    ModelFileNotFound(String),

    #[error("OpenAI model missing api_key")]
    MissingApiKey,

    #[error("failed to load model engine: {0}")]
    ModelLoadError(String),

    #[error("repository error: {0}")]
    Repository(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("user input error: {0}")]
    UserInput(String),
}

/// Configuration for infrastructure initialization
#[derive(Debug, Clone)]
pub struct InfrastructureConfig {
    pub debug: bool,
    pub app_name: String,
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            debug: false,
            app_name: "drastis".to_string(),
        }
    }
}

/// Result of infrastructure initialization
pub struct InfrastructureComponents {
    pub connection: Arc<Connection>,
    pub engine: Arc<dyn InferenceEngine>,
}

/// Infrastructure initialization service
/// Handles database setup, model loading, and inference engine initialization
pub struct InfrastructureInitializer {
    config: InfrastructureConfig,
}

impl InfrastructureInitializer {
    /// Create a new infrastructure initializer
    pub fn new(config: InfrastructureConfig) -> Self {
        Self { config }
    }

    /// Create with debug flag
    pub fn with_debug(debug: bool) -> Self {
        Self::new(InfrastructureConfig {
            debug,
            ..Default::default()
        })
    }

    /// Initialize all infrastructure components
    /// 1. Database connection
    /// 2. Model selection (if not already set)
    /// 3. Inference engine loading
    pub fn initialize(&self) -> Result<InfrastructureComponents, InitError> {
        // 1. Initialize database
        log::info!("Initializing database...");
        let connection = db::init_db(&self.config.app_name, self.config.debug)
            .map_err(|e| InitError::Database(e))?;

        // 2. Check if model is already selected
        let meta_repo = MetaRepository::new(&connection);
        let last_used_model_id = meta_repo
            .get_last_used_model_id()
            .map_err(|e| InitError::Repository(e))?;

        let engine = if let Some(model_id) = last_used_model_id {
            // Load existing model
            self.load_existing_model(&connection, model_id)?
        } else {
            // Prompt user to select a model
            self.select_and_setup_model(&connection)?
        };

        log::info!("Infrastructure initialized successfully.");

        Ok(InfrastructureComponents { connection, engine })
    }

    /// Load an existing model from the database
    fn load_existing_model(
        &self,
        conn: &Connection,
        model_id: i64,
    ) -> Result<Arc<dyn InferenceEngine>, InitError> {
        let models_repo = ModelsRepository::new(conn);
        let model = models_repo
            .find_by_id(model_id)
            .map_err(|e| InitError::Repository(e))?
            .ok_or(InitError::ModelNotFound(model_id))?;

        match model.model_type {
            ModelType::Local => {
                let gguf_path = model.gguf_file_path.ok_or(InitError::MissingGgufPath)?;
                let path = PathBuf::from(&gguf_path);
                // Canonicalize to ensure the path exists and is absolute
                let canonical_path = path
                    .canonicalize()
                    .map_err(|e| InitError::ModelFileNotFound(format!("{}: {}", gguf_path, e)))?;
                LocalEngine::load_with_path(&canonical_path)
                    .map_err(|e| InitError::ModelLoadError(e))
            }
            ModelType::OpenAI => {
                let api_key = model.api_key.ok_or(InitError::MissingApiKey)?;
                // Use saved model_name, fallback to gpt-4 for backward compatibility
                let model_name = model.model_name.as_deref().unwrap_or("gpt-4");
                use crate::infrastructure::inference::openai::OpenAIEngine;
                OpenAIEngine::new(&api_key, model_name).map_err(|e| InitError::ModelLoadError(e))
            }
        }
    }

    /// Prompt user to select and setup a model
    fn select_and_setup_model(
        &self,
        conn: &Connection,
    ) -> Result<Arc<dyn InferenceEngine>, InitError> {
        println!("\nNo model selected. Please choose a model type:");
        println!("  [1] Local (GGUF file)");
        println!("  [2] OpenAI");

        let model_type = loop {
            print!("Select model type (1-2): ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => break ModelType::Local,
                "2" => break ModelType::OpenAI,
                _ => println!("Invalid selection. Please enter 1 or 2."),
            }
        };

        let models_repo = ModelsRepository::new(conn);
        let meta_repo = MetaRepository::new(conn);

        let (model_id, engine) = match model_type {
            ModelType::Local => self.setup_local_model(conn, &models_repo, &meta_repo)?,
            ModelType::OpenAI => self.setup_openai_model(conn, &models_repo, &meta_repo)?,
        };

        // Set last_used_model_id - this must happen after successful engine loading
        meta_repo
            .set_last_used_model_id(model_id)
            .map_err(|e| InitError::Repository(e))?;

        Ok(engine)
    }

    /// Setup a local GGUF model
    fn setup_local_model(
        &self,
        conn: &Connection,
        models_repo: &ModelsRepository,
        meta_repo: &MetaRepository,
    ) -> Result<(i64, Arc<dyn InferenceEngine>), InitError> {
        // Scan for GGUF files
        let gguf_files = model_registry::scan_models()
            .map_err(|e| InitError::UserInput(format!("Failed to scan models: {}", e)))?;

        if gguf_files.is_empty() {
            return Err(InitError::UserInput(
                "No GGUF models found in ./models/ directory".to_string(),
            ));
        }

        // Display available models
        println!("\nAvailable GGUF models:");
        for (i, file) in gguf_files.iter().enumerate() {
            let filename = file.file_name().unwrap_or_default().to_string_lossy();
            println!("  [{}] {}", i + 1, filename);
        }

        // Prompt user to select
        let selected_file = loop {
            print!("Select model (1-{}): ", gguf_files.len());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= gguf_files.len() => {
                    break &gguf_files[n - 1];
                }
                _ => {
                    println!(
                        "Invalid selection. Please enter a number between 1 and {}.",
                        gguf_files.len()
                    );
                }
            }
        };

        // Convert to absolute path for consistency
        let gguf_path = selected_file
            .canonicalize()
            .unwrap_or_else(|_| selected_file.to_path_buf())
            .to_string_lossy()
            .to_string();

        // Check if model with this path already exists
        let existing_models = models_repo
            .find_by_type(ModelType::Local)
            .map_err(|e| InitError::Repository(e))?;

        let (model, engine) = if let Some(existing) = existing_models.iter().find(|m| {
            if let Some(existing_path) = &m.gguf_file_path {
                // Compare paths (handle both absolute and relative)
                let existing_abs = PathBuf::from(existing_path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(existing_path));
                let selected_abs = PathBuf::from(&gguf_path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(&gguf_path));
                existing_abs == selected_abs
            } else {
                false
            }
        }) {
            println!("Using existing model entry (id: {})", existing.id);
            // Verify the model file still exists
            let model_path = existing
                .gguf_file_path
                .as_ref()
                .map(PathBuf::from)
                .ok_or(InitError::MissingGgufPath)?;
            if !model_path.exists() {
                return Err(InitError::ModelFileNotFound(format!(
                    "{}. Please delete the model entry (id: {}) and try again.",
                    model_path.display(),
                    existing.id
                )));
            }
            // Load the engine for existing model
            let engine = LocalEngine::load_with_path(&model_path)
                .map_err(|e| InitError::ModelLoadError(e))?;
            (existing.clone(), engine)
        } else {
            // Try to load the engine FIRST before creating database entry
            // This ensures we don't create a partial entry if loading fails
            println!("Loading model (this may take a moment)...");
            let engine = LocalEngine::load_with_path(selected_file).map_err(|e| {
                InitError::ModelLoadError(format!(
                    "{}\n\
                    This could be due to:\n\
                    - Corrupted model file\n\
                    - Missing Metal framework support (macOS)\n\
                    - Insufficient memory\n\
                    Model entry was NOT created. Please check the model file and try again.",
                    e
                ))
            })?;

            // Only create database entry if engine loads successfully
            println!("Model loaded successfully. Creating database entry...");
            let model = models_repo
                .create(ModelType::Local, None, Some(&gguf_path), None)
                .map_err(|e| InitError::Repository(e))?;
            (model, engine)
        };

        Ok((model.id, engine))
    }

    /// Setup an OpenAI model
    fn setup_openai_model(
        &self,
        _conn: &Connection,
        models_repo: &ModelsRepository,
        _meta_repo: &MetaRepository,
    ) -> Result<(i64, Arc<dyn InferenceEngine>), InitError> {
        select_openai_model(models_repo, false)
    }
}

/// Shared OpenAI model selection logic
/// Used by both initial setup and runtime model change
fn select_openai_model(
    models_repo: &ModelsRepository,
    allow_cancel: bool,
) -> Result<(i64, Arc<dyn InferenceEngine>), InitError> {
    use crate::infrastructure::inference::openai::OpenAIEngine;

    // Check if there's an existing OpenAI model with API key
    let existing_models = models_repo
        .find_by_type(ModelType::OpenAI)
        .map_err(|e| InitError::Repository(e))?;

    // Get saved models that have a model_name
    let saved_models: Vec<_> = existing_models
        .iter()
        .filter(|m| m.model_name.is_some() && m.api_key.is_some())
        .collect();

    let api_key = if let Some(existing) = existing_models.iter().find(|m| m.api_key.is_some()) {
        println!("Using existing API key.");
        existing.api_key.clone().unwrap()
    } else {
        // Prompt for API key
        print!("Enter OpenAI API key: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        input.trim().to_string()
    };

    if api_key.is_empty() {
        return Err(InitError::UserInput("API key cannot be empty".to_string()));
    }

    // If we have saved models, show them first with option to fetch new
    let selected_model_name: String = if !saved_models.is_empty() {
        println!("\nSaved OpenAI models:");
        for (i, model) in saved_models.iter().enumerate() {
            println!("  [{}] {}", i + 1, model.model_name.as_ref().unwrap());
        }
        println!("  [{}] Fetch new model from API...", saved_models.len() + 1);

        let selection = loop {
            print!("Select option (1-{}): ", saved_models.len() + 1);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if allow_cancel && input.trim().is_empty() {
                println!("Cancelled.");
                return Err(InitError::UserInput("Cancelled".to_string()));
            }

            match input.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= saved_models.len() => {
                    // User selected an existing model
                    break Some(n - 1);
                }
                Ok(n) if n == saved_models.len() + 1 => {
                    // User wants to fetch new models from API
                    break None;
                }
                _ => {
                    println!(
                        "Invalid selection. Please enter a number between 1 and {}.",
                        saved_models.len() + 1
                    );
                }
            }
        };

        if let Some(idx) = selection {
            // Use existing saved model
            let model = saved_models[idx];
            let model_name = model.model_name.as_ref().unwrap();
            println!("Selected model: {}", model_name);

            let engine = OpenAIEngine::new(&api_key, model_name)
                .map_err(|e| InitError::ModelLoadError(e))?;

            return Ok((model.id, engine));
        }

        // Fall through to fetch from API
        fetch_and_select_model_from_api(&api_key, allow_cancel)?
    } else {
        // No saved models, fetch from API directly
        fetch_and_select_model_from_api(&api_key, allow_cancel)?
    };

    println!("Selected model: {}", selected_model_name);

    // Check if model with this model name already exists
    let model = if let Some(existing) = existing_models.iter().find(|m| {
        m.model_name
            .as_ref()
            .map(|n| n == &selected_model_name)
            .unwrap_or(false)
    }) {
        println!("Using existing model entry (id: {})", existing.id);
        existing.clone()
    } else {
        // Create new model entry with model_name
        println!("Creating new model entry...");
        models_repo
            .create(
                ModelType::OpenAI,
                Some(&api_key),
                None,
                Some(&selected_model_name),
            )
            .map_err(|e| InitError::Repository(e))?
    };

    // Load the engine with the selected model
    let engine = OpenAIEngine::new(&api_key, &selected_model_name)
        .map_err(|e| InitError::ModelLoadError(e))?;

    Ok((model.id, engine))
}

/// Fetch models from OpenAI API and let user select one
fn fetch_and_select_model_from_api(api_key: &str, allow_cancel: bool) -> Result<String, InitError> {
    use crate::infrastructure::inference::openai::OpenAIEngine;

    println!("Fetching available models from OpenAI...");
    let openai =
        OpenAIEngine::new_general(api_key, "gpt-4").map_err(|e| InitError::ModelLoadError(e))?;
    let available_models = openai
        .fetch_available_models()
        .map_err(|e| InitError::UserInput(format!("Failed to fetch models: {}", e)))?;

    if available_models.is_empty() {
        return Err(InitError::UserInput(
            "No compatible models found in your OpenAI account".to_string(),
        ));
    }

    // Display available models
    println!("\nAvailable OpenAI models:");
    for (i, model_name) in available_models.iter().enumerate() {
        println!("  [{}] {}", i + 1, model_name);
    }

    // Prompt user to select a model
    loop {
        print!("Select model (1-{}): ", available_models.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if allow_cancel && input.trim().is_empty() {
            println!("Cancelled.");
            return Err(InitError::UserInput("Cancelled".to_string()));
        }

        match input.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= available_models.len() => {
                return Ok(available_models[n - 1].clone());
            }
            _ => {
                println!(
                    "Invalid selection. Please enter a number between 1 and {}.",
                    available_models.len()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infrastructure_config() {
        let config = InfrastructureConfig::default();
        assert!(!config.debug);
        assert_eq!(config.app_name, "drastis");

        let initializer = InfrastructureInitializer::with_debug(true);
        assert!(initializer.config.debug);
    }
}

impl From<InitError> for String {
    fn from(err: InitError) -> Self {
        err.to_string()
    }
}

/// Get the current model name from the database
pub fn get_current_model_name(conn: &Connection) -> Result<String, String> {
    let meta_repo = MetaRepository::new(conn);
    let models_repo = ModelsRepository::new(conn);

    let model_id = meta_repo
        .get_last_used_model_id()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No model selected".to_string())?;

    let model = models_repo
        .find_by_id(model_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Model not found".to_string())?;

    // For OpenAI models, return the model_name if available
    // For local models, return the filename
    match model.model_type {
        ModelType::OpenAI => Ok(model.model_name.unwrap_or_else(|| "gpt-4".to_string())),
        ModelType::Local => model
            .gguf_file_path
            .map(|p| {
                std::path::Path::new(&p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p)
            })
            .ok_or_else(|| "Unknown local model".to_string()),
    }
}

/// Change the current model at runtime
/// Returns the new inference engine if successful
pub fn change_model(conn: &Connection) -> Result<Arc<dyn InferenceEngine>, InitError> {
    println!("\nChange model. Please choose a model type:");
    println!("  [1] Local (GGUF file)");
    println!("  [2] OpenAI");

    let model_type = loop {
        print!("Select model type (1-2): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim() {
            "1" => break ModelType::Local,
            "2" => break ModelType::OpenAI,
            "" => {
                println!("Cancelled.");
                return Err(InitError::UserInput("Cancelled".to_string()));
            }
            _ => println!("Invalid selection. Please enter 1 or 2."),
        }
    };

    let models_repo = ModelsRepository::new(conn);
    let meta_repo = MetaRepository::new(conn);

    let (model_id, engine) = match model_type {
        ModelType::Local => setup_local_model_runtime(conn, &models_repo)?,
        ModelType::OpenAI => setup_openai_model_runtime(conn, &models_repo)?,
    };

    // Update last_used_model_id
    meta_repo
        .set_last_used_model_id(model_id)
        .map_err(|e| InitError::Repository(e))?;

    println!("Model changed successfully.\n");

    Ok(engine)
}

/// Setup a local GGUF model (runtime version)
fn setup_local_model_runtime(
    conn: &Connection,
    models_repo: &ModelsRepository,
) -> Result<(i64, Arc<dyn InferenceEngine>), InitError> {
    // Scan for GGUF files
    let gguf_files = model_registry::scan_models()
        .map_err(|e| InitError::UserInput(format!("Failed to scan models: {}", e)))?;

    if gguf_files.is_empty() {
        return Err(InitError::UserInput(
            "No GGUF models found in ./models/ directory".to_string(),
        ));
    }

    // Display available models
    println!("\nAvailable GGUF models:");
    for (i, file) in gguf_files.iter().enumerate() {
        let filename = file.file_name().unwrap_or_default().to_string_lossy();
        println!("  [{}] {}", i + 1, filename);
    }

    // Prompt user to select
    let selected_file = loop {
        print!("Select model (1-{}): ", gguf_files.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().is_empty() {
            println!("Cancelled.");
            return Err(InitError::UserInput("Cancelled".to_string()));
        }

        match input.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= gguf_files.len() => {
                break &gguf_files[n - 1];
            }
            _ => {
                println!(
                    "Invalid selection. Please enter a number between 1 and {}.",
                    gguf_files.len()
                );
            }
        }
    };

    // Convert to absolute path for consistency
    let gguf_path = selected_file
        .canonicalize()
        .unwrap_or_else(|_| selected_file.to_path_buf())
        .to_string_lossy()
        .to_string();

    // Check if model with this path already exists
    let existing_models = models_repo
        .find_by_type(ModelType::Local)
        .map_err(|e| InitError::Repository(e))?;

    let (model, engine) = if let Some(existing) = existing_models.iter().find(|m| {
        if let Some(existing_path) = &m.gguf_file_path {
            let existing_abs = PathBuf::from(existing_path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(existing_path));
            let selected_abs = PathBuf::from(&gguf_path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(&gguf_path));
            existing_abs == selected_abs
        } else {
            false
        }
    }) {
        println!("Using existing model entry (id: {})", existing.id);
        let model_path = existing
            .gguf_file_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or(InitError::MissingGgufPath)?;
        if !model_path.exists() {
            return Err(InitError::ModelFileNotFound(format!(
                "{}. Please delete the model entry (id: {}) and try again.",
                model_path.display(),
                existing.id
            )));
        }
        let engine =
            LocalEngine::load_with_path(&model_path).map_err(|e| InitError::ModelLoadError(e))?;
        (existing.clone(), engine)
    } else {
        println!("Loading model (this may take a moment)...");
        let engine = LocalEngine::load_with_path(selected_file).map_err(|e| {
            InitError::ModelLoadError(format!(
                "{}\nModel entry was NOT created. Please check the model file and try again.",
                e
            ))
        })?;

        println!("Model loaded successfully. Creating database entry...");
        let model = models_repo
            .create(ModelType::Local, None, Some(&gguf_path), None)
            .map_err(|e| InitError::Repository(e))?;
        (model, engine)
    };

    Ok((model.id, engine))
}

/// Setup an OpenAI model (runtime version)
fn setup_openai_model_runtime(
    _conn: &Connection,
    models_repo: &ModelsRepository,
) -> Result<(i64, Arc<dyn InferenceEngine>), InitError> {
    select_openai_model(models_repo, true)
}
