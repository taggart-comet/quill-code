use crate::infrastructure::db;
use crate::infrastructure::inference::{InferenceEngine, InferenceParams};
use crate::infrastructure::model_registry;
use rusqlite::Connection;

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
    pub connection: Connection,
    pub engine: InferenceEngine,
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
    /// 2. Model scanning and selection
    /// 3. Inference engine loading
    pub fn initialize(&self) -> Result<InfrastructureComponents, String> {
        // 1. Initialize database
        log::info!("Initializing database...");
        let connection = db::init_db(&self.config.app_name, self.config.debug)?;

        // 2. Scan and select model
        log::info!("Scanning for GGUF models...");

        let models = model_registry::scan_models().map_err(|e| e.to_string())?;
        let selected = model_registry::select_model(models).map_err(|e| e.to_string())?;

        // 3. Load inference engine
        log::info!("Loading model...");
        
        let params = InferenceParams::default();
        log::debug!(
            "Parameters: ctx={}, temp={}, top_p={}, max_tokens={}, threads={}",
            params.ctx_size, params.temperature, params.top_p, params.max_tokens, params.threads
        );

        let engine = InferenceEngine::load(&selected, params, self.config.debug)?;
        
        log::info!("Infrastructure initialized successfully.");

        Ok(InfrastructureComponents {
            connection,
            engine,
        })
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
