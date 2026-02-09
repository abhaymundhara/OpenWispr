use crate::{
    prompts, FormattingType, LlmAdapter, LlmConfig, LlmError, Result, TextFormattingRequest,
    TextFormattingResponse,
};
use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::AddBos;
use std::num::NonZeroU32;
use std::sync::Arc;

pub struct LlamaCppAdapter {
    backend: Option<LlamaBackend>,
    model: Option<Arc<LlamaModel>>,
    config: Option<LlmConfig>,
    current_model_name: Option<String>,
}

impl LlamaCppAdapter {
    pub fn new() -> Self {
        Self {
            backend: None,
            model: None,
            config: None,
            current_model_name: None,
        }
    }

    fn ensure_initialized(&self) -> Result<()> {
        if self.model.is_none() {
            return Err(LlmError::ModelLoadError(
                "Adapter not initialized. Call initialize() first.".to_string(),
            ));
        }
        Ok(())
    }

    fn generate_response(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.ensure_initialized()?;

        let model = self
            .model
            .as_ref()
            .ok_or_else(|| LlmError::ModelLoadError("Model not loaded".to_string()))?;

        let backend = self
            .backend
            .as_ref()
            .ok_or_else(|| LlmError::ModelLoadError("Backend not initialized".to_string()))?;

        let config = self
            .config
            .as_ref()
            .ok_or_else(|| LlmError::ConfigError("Config not set".to_string()))?;

        // Create context params
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(512);

        let mut ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| LlmError::ModelLoadError(format!("Failed to create context: {}", e)))?;

        // Tokenize the prompt
        let tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LlmError::InferenceFailed(format!("Tokenization failed: {}", e)))?;

        // Prepare batch
        let mut batch = LlamaBatch::new(512, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| LlmError::InferenceFailed(format!("Batch add failed: {}", e)))?;
        }

        // Decode the prompt
        ctx.decode(&mut batch).map_err(|e| {
            LlmError::InferenceFailed(format!("Failed to decode prompt: {}", e))
        })?;

        let mut n_cur = batch.n_tokens();
        let mut output = String::new();
        let mut generated_tokens = 0;

        let last_index = (tokens.len() - 1) as i32;

        // Generate tokens
        while generated_tokens < max_tokens {
            // Sample next token using simple greedy sampling
            let candidates: Vec<_> = ctx.candidates_ith(last_index).collect();
            
            // Get the most likely token (first in candidates list is best)
            let token_id = if let Some(first) = candidates.first() {
                first.id()
            } else {
                break;
            };

            // Check for EOS
            if model.is_eog_token(token_id) {
                break;
            }

            // Decode token to string using updated API with encoder
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let piece = model.token_to_piece(token_id, &mut decoder, false, None)
                .map_err(|e| LlmError::InferenceFailed(format!("Token decode failed: {}", e)))?;

            output.push_str(&piece);

            // Prepare next batch
            batch.clear();
            batch
                .add(token_id, n_cur, &[0], true)
                .map_err(|e| LlmError::InferenceFailed(format!("Batch add failed: {}", e)))?;

            // Decode next token
            ctx.decode(&mut batch).map_err(|e| {
                LlmError::InferenceFailed(format!("Failed to decode token: {}", e))
            })?;

            n_cur += 1;
            generated_tokens += 1;
        }

        Ok(output.trim().to_string())
    }
}

#[async_trait]
impl LlmAdapter for LlamaCppAdapter {
    async fn initialize(&mut self, config: LlmConfig) -> Result<()> {
        tracing::info!("Initializing LlamaCpp adapter with model: {}", config.model_name);

        // Get model path
        let model_path = if let Some(path) = &config.model_path {
            path.clone()
        } else {
            crate::models::get_model_path(&config.model_name)?
        };

        if !model_path.exists() {
            return Err(LlmError::ModelNotFound(format!(
                "Model file not found at {:?}. Please download it first.",
                model_path
            )));
        }

        // Initialize backend
        let backend = LlamaBackend::init().map_err(|e| {
            LlmError::ModelLoadError(format!("Failed to initialize llama backend: {}", e))
        })?;

        // Configure model params (Metal for macOS, CUDA/Vulkan for Windows auto-detected)
        let model_params = LlamaModelParams::default();

        // Load model
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params).map_err(
            |e| LlmError::ModelLoadError(format!("Failed to load model: {}", e)),
        )?;

        tracing::info!("Model loaded successfully: {}", config.model_name);

        self.backend = Some(backend);
        self.model = Some(Arc::new(model));
        self.config = Some(config.clone());
        self.current_model_name = Some(config.model_name.clone());

        Ok(())
    }

    async fn format_text(&self, request: TextFormattingRequest) -> Result<TextFormattingResponse> {
        self.ensure_initialized()?;

        // Create prompt based on formatting type
        let prompt = match request.format_type {
            FormattingType::RemoveFillers => prompts::create_filler_removal_prompt(&request.raw_text),
            FormattingType::AddPunctuation => prompts::create_punctuation_prompt(&request.raw_text),
            FormattingType::FixCapitalization => prompts::create_capitalization_prompt(&request.raw_text),
            FormattingType::CourseCorrection => prompts::create_course_correction_prompt(&request.raw_text),
            FormattingType::SmartFormat => prompts::create_smart_format_prompt(&request.raw_text),
        };

        let config = self
            .config
            .as_ref()
            .ok_or_else(|| LlmError::ConfigError("Config not set".to_string()))?;

        // Generate formatted text
        let formatted_text = self.generate_response(&prompt, config.max_tokens)?;

        Ok(TextFormattingResponse {
            formatted_text,
            original_text: request.raw_text,
        })
    }

    async fn run_prompt(&self, prompt: String, max_tokens: u32) -> Result<String> {
        self.ensure_initialized()?;
        self.generate_response(&prompt, max_tokens)
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        crate::models::is_model_downloaded(model_name)
    }

    fn current_model(&self) -> Option<String> {
        self.current_model_name.clone()
    }
}
