//! Windows STT adapter.
//! Uses local whisper.cpp backend.

use crate::{AudioFormat, Result, SttAdapter, SttConfig, Transcription};
use async_trait::async_trait;
use tracing::info;

use super::backend::SharedWhisperAdapter;

pub struct WhisperAdapter {
    inner: SharedWhisperAdapter,
}

impl WhisperAdapter {
    pub fn new() -> Self {
        info!("Initializing Windows whisper adapter");
        Self {
            inner: SharedWhisperAdapter::new("Windows whisper backend"),
        }
    }
}

#[async_trait]
impl SttAdapter for WhisperAdapter {
    async fn initialize(&mut self, config: SttConfig) -> Result<()> {
        self.inner.initialize(config).await
    }

    async fn transcribe(&self, audio_data: &[f32], format: AudioFormat) -> Result<Transcription> {
        self.inner.transcribe(audio_data, format).await
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        self.inner.is_model_available(model_name).await
    }

    fn available_models(&self) -> Vec<String> {
        self.inner.available_models()
    }

    fn current_model(&self) -> Option<String> {
        self.inner.current_model()
    }
}

impl Default for WhisperAdapter {
    fn default() -> Self {
        Self::new()
    }
}
