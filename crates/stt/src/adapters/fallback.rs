//! Fallback STT adapter for when no backend is available.
//! Provides a no-op implementation to maintain compilation.

use crate::{AudioFormat, Result, SttAdapter, SttConfig, Transcription, SttError};
use async_trait::async_trait;
use tracing::warn;

#[derive(Default)]
pub struct FallbackAdapter;

impl FallbackAdapter {
    pub fn new() -> Self {
        warn!("Initializing fallback STT adapter (no backend available)");
        Self
    }
}

#[async_trait]
impl SttAdapter for FallbackAdapter {
    async fn initialize(&mut self, _config: SttConfig) -> Result<()> {
        Ok(())
    }

    async fn transcribe(&self, _audio_data: &[f32], _format: AudioFormat) -> Result<Transcription> {
        Err(SttError::TranscriptionFailed("No STT backend available on this system".to_string()))
    }

    async fn is_model_available(&self, _model_name: &str) -> bool {
        false
    }

    fn available_models(&self) -> Vec<String> {
        vec![]
    }

    fn current_model(&self) -> Option<String> {
        None
    }
}
