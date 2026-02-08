//! Speech-to-Text adapter layer
//! Provides a unified interface for different STT backends (MLX, whisper.cpp, etc.)

use async_trait::async_trait;
use std::path::PathBuf;
use thiserror::Error;

pub mod adapters;

/// STT-specific errors
#[derive(Debug, Error)]
pub enum SttError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("Audio processing error: {0}")]
    AudioError(String),

    #[error("Model loading error: {0}")]
    ModelLoadError(String),

    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, SttError>;

/// Audio format specification
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 16000, // Whisper expects 16kHz
            channels: 1,        // Mono
            bits_per_sample: 16,
        }
    }
}

/// Transcription result with optional metadata
#[derive(Debug, Clone)]
pub struct Transcription {
    pub text: String,
    pub language: Option<String>,
    pub confidence: Option<f32>,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Configuration for STT models
#[derive(Debug, Clone)]
pub struct SttConfig {
    pub model_name: String,
    pub model_path: Option<PathBuf>,
    pub language: Option<String>,
    pub task: TranscriptionTask,
}

#[derive(Debug, Clone)]
pub enum TranscriptionTask {
    Transcribe,
    Translate, // Translate to English
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model_name: "base".to_string(),
            model_path: None,
            language: None,
            task: TranscriptionTask::Transcribe,
        }
    }
}

/// Core STT adapter trait - implemented by platform-specific backends
#[async_trait]
pub trait SttAdapter: Send + Sync {
    /// Initialize the adapter and load the model
    async fn initialize(&mut self, config: SttConfig) -> Result<()>;

    /// Transcribe audio data to text
    async fn transcribe(&self, audio_data: &[f32], format: AudioFormat) -> Result<Transcription>;

    /// Check if a model is available/downloaded
    async fn is_model_available(&self, model_name: &str) -> bool;

    /// List available models
    fn available_models(&self) -> Vec<String>;

    /// Get the current model name
    fn current_model(&self) -> Option<String>;
}

/// Factory function to create the appropriate STT adapter for the current platform
pub fn create_adapter() -> Result<Box<dyn SttAdapter>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(adapters::mlx::MlxAdapter::new()))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(adapters::whisper::WhisperAdapter::new()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(SttError::UnsupportedPlatform)
    }
}
