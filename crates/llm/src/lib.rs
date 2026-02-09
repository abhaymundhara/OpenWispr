//! LLM adapter layer for local and remote language models
//! Provides text formatting and processing capabilities

use async_trait::async_trait;
use std::path::PathBuf;
use thiserror::Error;

pub mod adapters;
pub mod models;
pub mod prompts;

pub use models::{LlmModelInfo, list_models, download_model, get_model_path, is_model_downloaded};

/// LLM-specific errors
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Model loading error: {0}")]
    ModelLoadError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Unsupported platform")]
    UnsupportedPlatform,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Model error: {0}")]
    ModelError(#[from] models::ModelError),
}

pub type Result<T> = std::result::Result<T, LlmError>;

/// Configuration for LLM inference
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model_name: String,
    pub model_path: Option<PathBuf>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub top_p: f32,
    pub top_k: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_name: "SmolLM2-135M-Instruct-Q4_K_M".to_string(),
            model_path: None,
            temperature: 0.3, // Lower for more deterministic formatting
            max_tokens: 512,  // Reasonable for text formatting
            top_p: 0.9,
            top_k: 40,
        }
    }
}

/// Request for text formatting
#[derive(Debug, Clone)]
pub struct TextFormattingRequest {
    pub raw_text: String,
    pub format_type: FormattingType,
}

/// Types of text formatting operations
#[derive(Debug, Clone)]
pub enum FormattingType {
    RemoveFillers,
    AddPunctuation,
    FixCapitalization,
    CourseCorrection,
    SmartFormat, // All-in-one
}

/// Response from text formatting
#[derive(Debug, Clone)]
pub struct TextFormattingResponse {
    pub formatted_text: String,
    pub original_text: String,
}

/// Core LLM adapter trait - implemented by different backends
#[async_trait]
pub trait LlmAdapter: Send + Sync {
    /// Initialize the adapter and load the model
    async fn initialize(&mut self, config: LlmConfig) -> Result<()>;

    /// Format text using the LLM
    async fn format_text(&self, request: TextFormattingRequest) -> Result<TextFormattingResponse>;

    /// Run a custom prompt
    async fn run_prompt(&self, prompt: String, max_tokens: u32) -> Result<String>;

    /// Check if a model is available/downloaded
    async fn is_model_available(&self, model_name: &str) -> bool;

    /// Get the current model name
    fn current_model(&self) -> Option<String>;
}

/// Factory function to create the appropriate LLM adapter
pub fn create_adapter() -> Result<Box<dyn LlmAdapter>> {
    // Both macOS and Windows use llama.cpp
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Ok(Box::new(adapters::LlamaCppAdapter::new()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(LlmError::UnsupportedPlatform)
    }
}
