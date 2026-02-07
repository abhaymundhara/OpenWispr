//! macOS STT adapter.
//! Uses local whisper.cpp backend for now to provide production STT.

use crate::{AudioFormat, Result, SttAdapter, SttConfig, Transcription};
use async_trait::async_trait;
use tracing::{info, warn};

use super::backend::SharedWhisperAdapter;

pub struct MlxAdapter {
    inner: SharedWhisperAdapter,
}

impl MlxAdapter {
    pub fn new() -> Self {
        info!("Initializing macOS STT adapter");
        Self {
            inner: SharedWhisperAdapter::new("macOS whisper backend"),
        }
    }

    fn is_apple_silicon() -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            true
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            false
        }
    }
}

#[async_trait]
impl SttAdapter for MlxAdapter {
    async fn initialize(&mut self, config: SttConfig) -> Result<()> {
        if !Self::is_apple_silicon() {
            warn!("Running on Intel Mac: STT works but without Apple Silicon optimization");
        }
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

impl Default for MlxAdapter {
    fn default() -> Self {
        Self::new()
    }
}
