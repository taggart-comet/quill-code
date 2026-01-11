use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};
use std::io::{self, Write};
use std::num::NonZeroU32;
use std::path::Path;

pub struct InferenceParams {
    pub ctx_size: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    pub threads: i32,
}

impl Default for InferenceParams {
    fn default() -> Self {
        let cpu_count = num_cpus::get() as i32;
        Self {
            ctx_size: 4096,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 512,
            threads: (cpu_count - 1).max(1),
        }
    }
}

pub struct InferenceEngine {
    model: LlamaModel,
    backend: LlamaBackend,
    params: InferenceParams,
}

impl InferenceEngine {
    pub fn load<P: AsRef<Path>>(
        model_path: P,
        params: InferenceParams,
        debug: bool,
    ) -> Result<Self, String> {
        // Configure logging - suppress unless debug mode
        let log_options = LogOptions::default().with_logs_enabled(debug);
        send_logs_to_tracing(log_options);

        let backend =
            LlamaBackend::init().map_err(|e| format!("Failed to init backend: {}", e))?;

        let model_params = LlamaModelParams::default();

        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {}", e))?;

        Ok(Self {
            model,
            backend,
            params,
        })
    }

    pub fn generate(&self, prompt: &str) -> Result<String, String> {
        self.generate_internal(prompt, true)
    }

    /// Generate without printing to stdout (for internal use like session naming)
    pub fn generate_silent(&self, prompt: &str, max_tokens: u32) -> Result<String, String> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.params.ctx_size))
            .with_n_threads(self.params.threads)
            .with_n_threads_batch(self.params.threads);

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {}", e))?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Failed to tokenize: {}", e))?;

        if tokens.is_empty() {
            return Err("Empty prompt".to_string());
        }

        let mut batch = LlamaBatch::new(self.params.ctx_size as usize, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|_| "Failed to add token to batch")?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| format!("Initial decode failed: {}", e))?;

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
                .map_err(|e| format!("Token decode error: {}", e))?;

            output.push_str(&token_str);

            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|_| "Failed to add token")?;

            ctx.decode(&mut batch)
                .map_err(|e| format!("Decode failed: {}", e))?;

            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }

    fn generate_internal(&self, prompt: &str, stream: bool) -> Result<String, String> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.params.ctx_size))
            .with_n_threads(self.params.threads)
            .with_n_threads_batch(self.params.threads);

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {}", e))?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Failed to tokenize: {}", e))?;

        if tokens.is_empty() {
            return Err("Empty prompt".to_string());
        }

        let mut batch = LlamaBatch::new(self.params.ctx_size as usize, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|_| "Failed to add token to batch")?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| format!("Initial decode failed: {}", e))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(self.params.temperature),
            LlamaSampler::top_p(self.params.top_p, 1),
            LlamaSampler::dist(0),
        ]);

        let mut output = String::new();
        let mut n_cur = tokens.len();

        if stream {
            print!("\n");
            io::stdout().flush().ok();
        }

        for _ in 0..self.params.max_tokens {
            let new_token = sampler.sample(&ctx, -1);

            if self.model.is_eog_token(new_token) {
                break;
            }

            let token_str = self
                .model
                .token_to_str(new_token, Special::Tokenize)
                .map_err(|e| format!("Token decode error: {}", e))?;

            if stream {
                print!("{}", token_str);
                io::stdout().flush().ok();
            }
            output.push_str(&token_str);

            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|_| "Failed to add token")?;

            ctx.decode(&mut batch)
                .map_err(|e| format!("Decode failed: {}", e))?;

            n_cur += 1;
        }

        if stream {
            println!("\n");
        }

        Ok(output)
    }
}
