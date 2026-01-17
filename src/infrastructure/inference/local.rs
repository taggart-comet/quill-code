use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once, OnceLock};

use super::{InferenceEngine, LLMInferenceResult};
use crate::domain::ModelType;
use crate::infrastructure::InfaError;
use crate::infrastructure::model_registry;

pub struct LocalParams {
    pub ctx_size: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub threads: i32,
}

impl Default for LocalParams {
    fn default() -> Self {
        let cpu_count = num_cpus::get() as i32;
        Self {
            ctx_size: 4096,
            temperature: 0.7,
            top_p: 0.9,
            threads: (cpu_count - 1).max(1),
        }
    }
}

// Silence llama.cpp verbose logging
fn silence_llama_logs() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));
    });
}

// Global singleton cache for inference engines per model path
fn engine_cache() -> &'static Mutex<HashMap<PathBuf, Arc<LocalEngine>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<LocalEngine>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct LocalEngine {
    model: LlamaModel,
    params: LocalParams,
}

impl LocalEngine {
    /// Load or get a cached inference engine instance.
    /// Automatically scans and selects a model from the registry.
    pub fn load() -> Result<Arc<dyn InferenceEngine>, String> {
        log::info!("Scanning for GGUF models...");
        let models =
            model_registry::scan_models().map_err(|e| format!("Failed to scan models: {}", e))?;
        let selected = model_registry::select_model(models)
            .map_err(|e| format!("Failed to select model: {}", e))?;

        Self::load_with_path(&selected)
    }

    /// Load or get a cached inference engine instance for a specific model path.
    pub fn load_with_path<P: AsRef<Path>>(
        model_path: P,
    ) -> Result<Arc<dyn InferenceEngine>, String> {
        silence_llama_logs();
        let model_path = model_path.as_ref().to_path_buf();

        // Return cached engine if available
        if let Some(engine) = engine_cache().lock().unwrap().get(&model_path) {
            log::debug!("Using cached inference engine for model: {:?}", model_path);
            return Ok(Arc::clone(engine) as Arc<dyn InferenceEngine>);
        }

        // Create new engine
        log::info!("Loading model: {:?}", model_path);
        let backend =
            LlamaBackend::init().map_err(|e| format!("Failed to initialize backend: {}", e))?;
        let model_params = LlamaModelParams::default().with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {}", e))?;

        let engine = Arc::new(Self {
            model,
            params: LocalParams::default(),
        });

        // Cache and return
        engine_cache()
            .lock()
            .unwrap()
            .insert(model_path, Arc::clone(&engine));
        Ok(engine as Arc<dyn InferenceEngine>)
    }
}

impl InferenceEngine for LocalEngine {
    fn generate(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
        _tools: &[&dyn crate::domain::tools::Tool],
        _chain: &crate::domain::workflow::Chain,
    ) -> Result<LLMInferenceResult, InfaError> {
        let prompt = format!("{}\n\n{}", system_prompt, user_prompt);
        let to_error = |msg: String| -> InfaError {
            std::io::Error::new(std::io::ErrorKind::Other, msg).into()
        };
        let backend =
            LlamaBackend::init().map_err(|e| to_error(format!("Failed to initialize backend: {}", e)))?;

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.params.ctx_size))
            .with_n_threads(self.params.threads)
            .with_n_threads_batch(self.params.threads);

        let mut ctx = self
            .model
            .new_context(&backend, ctx_params)
            .map_err(|e| to_error(format!("Failed to create context: {}", e)))?;

        let tokens = self
            .model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| to_error(format!("Failed to tokenize: {}", e)))?;

        if tokens.is_empty() {
            return Err(to_error("Empty prompt".to_string()));
        }

        let mut batch = LlamaBatch::new(self.params.ctx_size as usize, 1);
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|_| to_error("Failed to add token to batch".to_string()))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| to_error(format!("Initial decode failed: {}", e)))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(self.params.temperature),
            LlamaSampler::top_p(self.params.top_p, 1),
            LlamaSampler::dist(0),
        ]);

        let mut output = String::new();
        let mut n_cur = tokens.len();

        for _ in 0..max_tokens {
            let new_token = sampler.sample(&ctx, -1);

            if self.model.is_eog_token(new_token) {
                break;
            }

            let token_str = self
                .model
                .token_to_str(new_token, Special::Tokenize)
                .map_err(|e| to_error(format!("Token decode error: {}", e)))?;
            output.push_str(&token_str);

            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|_| to_error("Failed to add token".to_string()))?;

            ctx.decode(&mut batch)
                .map_err(|e| to_error(format!("Decode failed: {}", e)))?;

            n_cur += 1;
        }

        Ok(LLMInferenceResult {
            summary: output.trim().to_string(),
            chosen_tool: None,
        })
    }

    fn get_type(&self) -> ModelType {
        ModelType::Local
    }
}
