//! whisper.cpp-based STT adapter for Windows
//! Uses the fast C++ implementation of OpenAI Whisper

use crate::{
    AudioFormat, Result, SttAdapter, SttConfig, SttError, Transcription, TranscriptSegment,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Whisper.cpp adapter state
pub struct WhisperAdapter {
    config: Arc<RwLock<Option<SttConfig>>>,
    // TODO: Add whisper.cpp context handle when integrating bindings
    // context: Option<WhisperContext>,
}

impl WhisperAdapter {
    pub fn new() -> Self {
        info!("Initializing whisper.cpp adapter for Windows");
        Self {
            config: Arc::new(RwLock::new(None)),
        }
    }

    /// Check for CUDA availability for GPU acceleration
    fn check_cuda_available() -> bool {
        // TODO: Implement CUDA detection
        // This will check for NVIDIA GPU and CUDA runtime
        debug!("Checking for CUDA availability");
        false
    }
}

#[async_trait]
impl SttAdapter for WhisperAdapter {
    async fn initialize(&mut self, config: SttConfig) -> Result<()> {
        info!("Initializing whisper.cpp adapter with model: {}", config.model_name);
        
        let has_cuda = Self::check_cuda_available();
        if has_cuda {
            info!("CUDA detected - will use GPU acceleration");
        } else {
            info!("No CUDA detected - using CPU");
        }

        // TODO: Load whisper.cpp model
        // This will use:
        // 1. whisper-rs Rust bindings to whisper.cpp
        // 2. Direct FFI to whisper.cpp library
        // 3. Load GGML model file from disk
        
        // For now, store config
        *self.config.write().await = Some(config);
        
        debug!("whisper.cpp adapter initialized (stub implementation)");
        Ok(())
    }

    async fn transcribe(&self, audio_data: &[f32], _format: AudioFormat) -> Result<Transcription> {
        let config = self.config.read().await;
        let config = config.as_ref().ok_or_else(|| {
            SttError::TranscriptionFailed("Adapter not initialized".to_string())
        })?;

        info!(
            "Transcribing {} samples with whisper.cpp (model: {})",
            audio_data.len(),
            config.model_name
        );

        // TODO: Implement actual whisper.cpp transcription
        // Steps:
        // 1. Ensure audio is 16kHz mono (resample if needed)
        // 2. Pass audio to whisper.cpp context
        // 3. Run transcription
        // 4. Parse segments and return results
        
        // Stub implementation for now
        warn!("whisper.cpp transcription not yet implemented - returning stub");
        Ok(Transcription {
            text: "[Whisper.cpp Stub] Transcription will appear here".to_string(),
            language: Some("en".to_string()),
            confidence: Some(0.92),
            segments: vec![TranscriptSegment {
                text: "[Whisper.cpp Stub] Transcription will appear here".to_string(),
                start: 0.0,
                end: 1.0,
            }],
        })
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        // TODO: Check if GGML model file exists in models directory
        debug!("Checking if whisper.cpp model '{}' is available", model_name);
        
        // For now, assume base models are available
        matches!(model_name, "tiny" | "base" | "small" | "medium" | "large")
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "tiny".to_string(),
            "tiny.en".to_string(),
            "base".to_string(),
            "base.en".to_string(),
            "small".to_string(),
            "small.en".to_string(),
            "medium".to_string(),
            "medium.en".to_string(),
            "large".to_string(),
        ]
    }

    fn current_model(&self) -> Option<String> {
        // Use blocking read since this is a sync function
        self.config.blocking_read().as_ref().map(|c| c.model_name.clone())
    }
}

impl Default for WhisperAdapter {
    fn default() -> Self {
        Self::new()
    }
}
