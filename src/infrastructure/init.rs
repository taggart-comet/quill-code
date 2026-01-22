use crate::domain::ModelType;
use crate::infrastructure::db;
use crate::infrastructure::inference::openai::OpenAIEngine;
use crate::infrastructure::inference::{local::LocalEngine, InferenceEngine};
use crate::infrastructure::app_bus::{LocalModelInfo, ModelSelection};
use crate::infrastructure::model_registry;
use crate::repository::{MetaRepository, ModelsRepository, UserSettingsRepository};
use rusqlite::Connection;
use std::io::{self};
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
    pub _debug: bool,
    pub app_name: String,
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            _debug: false,
            app_name: "drastis".to_string(),
        }
    }
}

/// Result of infrastructure initialization
pub struct InfrastructureComponents {
    pub connection: Arc<Connection>,
    pub engine: Option<Arc<dyn InferenceEngine>>,
    pub app_name: String,
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

    pub fn with_debug(debug: bool) -> Self {
        Self::new(InfrastructureConfig {
            _debug: debug,
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
        let connection = db::init_db(&self.config.app_name)
            .map_err(|e| InitError::Database(e))?;

        // 2. Check if model is already selected
        let meta_repo = MetaRepository::new(&connection);
        let last_used_model_id = meta_repo
            .get_last_used_model_id()
            .map_err(|e| InitError::Repository(e))?;

        let engine = if let Some(model_id) = last_used_model_id {
            let engine = self.load_existing_model(&connection, model_id)?;
            let settings_repo = UserSettingsRepository::new(&connection);
            let _ = settings_repo.update_current_model_id(Some(model_id));
            Some(engine)
        } else {
            None
        };

        log::info!("Infrastructure initialized successfully.");

        Ok(InfrastructureComponents {
            connection,
            engine,
            app_name: self.config.app_name.clone(),
        })
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
                let settings_repo = UserSettingsRepository::new(conn);
                let settings = settings_repo
                    .get_current()
                    .map_err(InitError::Repository)?;
                let api_key = settings.openai_api_key.ok_or(InitError::MissingApiKey)?;
                // Use saved model_name, fallback to gpt-4 for backward compatibility
                let model_name = model.model_name.as_deref().unwrap_or("gpt-4");
                use crate::infrastructure::inference::openai::OpenAIEngine;
                OpenAIEngine::new(&api_key, model_name).map_err(|e| InitError::ModelLoadError(e))
            }
        }
    }

}

impl Default for InfrastructureInitializer {
    fn default() -> Self {
        Self::new(InfrastructureConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infrastructure_config() {
        let config = InfrastructureConfig::default();
        assert_eq!(config.app_name, "drastis");
        let initializer = InfrastructureInitializer::default();
        assert_eq!(initializer.config.app_name, "drastis");
    }
}

impl From<InitError> for String {
    fn from(err: InitError) -> Self {
        err.to_string()
    }
}

/// Get the current model name from the database
pub fn get_current_model_name(conn: &Connection) -> Result<String, String> {
    let models_repo = ModelsRepository::new(conn);
    let settings_repo = UserSettingsRepository::new(conn);

    let settings = settings_repo.get_current().map_err(|e| e.to_string())?;
    let model_id = settings
        .current_model_id
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

pub fn list_local_models() -> Result<Vec<LocalModelInfo>, InitError> {
    let gguf_files = model_registry::scan_models()
        .map_err(|e| InitError::UserInput(format!("Failed to scan models: {}", e)))?;

    let models = gguf_files
        .iter()
        .map(|file| {
            let filename = file.file_name().unwrap_or_default().to_string_lossy();
            let path = file
                .canonicalize()
                .unwrap_or_else(|_| file.to_path_buf())
                .to_string_lossy()
                .to_string();
            LocalModelInfo {
                name: filename.to_string(),
                path,
            }
        })
        .collect();

    Ok(models)
}

pub fn update_openai_api_key(conn: &Connection, api_key: &str) -> Result<(), InitError> {
    let settings_repo = UserSettingsRepository::new(conn);
    settings_repo
        .update_openai_api_key(Some(api_key))
        .map_err(InitError::Repository)
}

pub fn apply_model_selection(
    conn: &Connection,
    selection: ModelSelection,
) -> Result<Arc<dyn InferenceEngine>, InitError> {
    let models_repo = ModelsRepository::new(conn);
    let meta_repo = MetaRepository::new(conn);
    let settings_repo = UserSettingsRepository::new(conn);

    match selection {
        ModelSelection::LocalPath(path) => {
            let path_buf = PathBuf::from(&path);
            if !path_buf.exists() {
                return Err(InitError::ModelFileNotFound(path));
            }
            let canonical_path = path_buf
                .canonicalize()
                .unwrap_or_else(|_| path_buf.clone());
            let gguf_path = canonical_path.to_string_lossy().to_string();

            let existing_models = models_repo
                .find_by_type(ModelType::Local)
                .map_err(InitError::Repository)?;

            let mut model_id = None;
            for model in existing_models {
                if let Some(existing_path) = &model.gguf_file_path {
                    let existing_abs = PathBuf::from(existing_path)
                        .canonicalize()
                        .unwrap_or_else(|_| PathBuf::from(existing_path));
                    if existing_abs == canonical_path {
                        model_id = Some(model.id);
                        break;
                    }
                }
            }

            let model_id = match model_id {
                Some(id) => id,
                None => {
                    let model = models_repo
                        .create(ModelType::Local, None, Some(&gguf_path), None)
                        .map_err(InitError::Repository)?;
                    model.id
                }
            };

            let engine = LocalEngine::load_with_path(&canonical_path)
                .map_err(InitError::ModelLoadError)?;

            meta_repo
                .set_last_used_model_id(model_id)
                .map_err(InitError::Repository)?;

            settings_repo
                .update_current_model_id(Some(model_id))
                .map_err(InitError::Repository)?;

            Ok(engine)
        }
        ModelSelection::OpenAiModel(model_name) => {
            let existing_models = models_repo
                .find_by_type(ModelType::OpenAI)
                .map_err(InitError::Repository)?;

            let settings = settings_repo
                .get_current()
                .map_err(InitError::Repository)?;
            let resolved_api_key = settings.openai_api_key.ok_or(InitError::MissingApiKey)?;

            let mut model_id = None;
            for model in &existing_models {
                if model.model_name.as_deref() == Some(&model_name) {
                    model_id = Some(model.id);
                    break;
                }
            }

            if model_id.is_none() {
                if let Some(existing) = existing_models.iter().find(|model| model.model_name.is_none())
                {
                    models_repo
                        .update_model_name(existing.id, Some(&model_name))
                        .map_err(InitError::Repository)?;
                    model_id = Some(existing.id);
                }
            }

            let model_id = match model_id {
                Some(id) => id,
                None => models_repo
                    .create(
                        ModelType::OpenAI,
                        None,
                        None,
                        Some(&model_name),
                    )
                    .map_err(InitError::Repository)?
                    .id,
            };

            let engine = OpenAIEngine::new(&resolved_api_key, &model_name)
                .map_err(InitError::ModelLoadError)?;

            meta_repo
                .set_last_used_model_id(model_id)
                .map_err(InitError::Repository)?;

            settings_repo
                .update_current_model_id(Some(model_id))
                .map_err(InitError::Repository)?;

            Ok(engine)
        }
    }
}
