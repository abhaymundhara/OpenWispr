//! MLX-based STT adapter for macOS (Apple Silicon)
//! Uses MLX framework optimized for Apple's Metal and Neural Engine

use crate::{
    AudioFormat, Result, SttAdapter, SttConfig, SttError, Transcription, TranscriptSegment,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// MLX adapter state
pub struct MlxAdapter {
    config: Arc<RwLock<Option<SttConfig>>>,
    // TODO: Add MLX model handle when integrating Python/Swift bindings
    // model: Option<MlxWhisperModel>,
}

impl MlxAdapter {
    pub fn new() -> Self {
        info!("Initializing MLX adapter for macOS");
        Self {
            config: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if running on Apple Silicon
    fn is_apple_silicon() -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            true
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            warn!("MLX adapter works best on Apple Silicon (M1/M2/M3)");
            false
        }
    }
}

#[async_trait]
impl SttAdapter for MlxAdapter {
    async fn initialize(&mut self, config: SttConfig) -> Result<()> {
        info!("Initializing MLX adapter with model: {}", config.model_name);
        
        if !Self::is_apple_silicon() {
            warn!("Running on Intel Mac - MLX performance will be limited");
        }

        // TODO: Load MLX Whisper model
        // This will use either:
        // 1. Python bindings to mlx-whisper package
        // 2. Swift/Objective-C bridge to MLX framework
        // 3. Rust FFI to compiled MLX library
        
        // For now, store config
        *self.config.write().await = Some(config);
        
        debug!("MLX adapter initialized (stub implementation)");
        Ok(())
    }

    async fn transcribe(&self, audio_data: &[f32], _format: AudioFormat) -> Result<Transcription> {
        let config = self.config.read().await;
        let config = config.as_ref().ok_or_else(|| {
            SttError::TranscriptionFailed("Adapter not initialized".to_string())
        })?;

        info!(
            "Transcribing {} samples with MLX (model: {})",
            audio_data.len(),
            config.model_name
        );

        // TODO: Implement actual MLX transcription
        // Steps:
        // 1. Convert f32 audio to the format MLX expects
        // 2. Call MLX Whisper model
        // 3. Parse and return transcription results
        
        // Stub implementation for now
        warn!("MLX transcription not yet implemented - returning stub");
        Ok(Transcription {
            text: "[MLX Stub] Transcription will appear here".to_string(),
            language: Some("en".to_string()),
            confidence: Some(0.95),
            segments: vec![TranscriptSegment {
                text: "[MLX Stub] Transcription will appear here".to_string(),
                start: 0.0,
                end: 1.0,
            }],
        })
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        // TODO: Check if MLX model is downloaded
        debug!("Checking if MLX model '{}' is available", model_name);
        
        // For now, assume base models are available
        matches!(model_name, "tiny" | "base" | "small" | "medium" | "large")
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "tiny".to_string(),
            "base".to_string(),
            "small".to_string(),
            "medium".to_string(),
            "large".to_string(),
        ]
    }

    fn current_model(&self) -> Option<String> {
        // Use blocking read since this is a sync function
        self.config.blocking_read().as_ref().map(|c| c.model_name.clone())
    }
}

impl Default for MlxAdapter {
    fn default() -> Self {
        Self::new()
    }
}
