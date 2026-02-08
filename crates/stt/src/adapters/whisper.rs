//! Windows STT adapter.
//! Routes between whisper.cpp and Sherpa ONNX based on selected model.

use crate::{
    is_sherpa_model_name, AudioFormat, Result, SttAdapter, SttConfig, SttError, Transcription,
};
use async_trait::async_trait;
use tracing::info;

use super::backend::SharedWhisperAdapter;
use super::sherpa::SharedSherpaAdapter;
use std::sync::{Arc, Mutex};

pub struct WhisperAdapter {
    whisper: SharedWhisperAdapter,
    sherpa: SharedSherpaAdapter,
    current_model: Arc<Mutex<Option<String>>>,
}

impl WhisperAdapter {
    pub fn new() -> Self {
        info!("Initializing Windows whisper adapter");
        Self {
            whisper: SharedWhisperAdapter::new("Windows whisper backend"),
            sherpa: SharedSherpaAdapter::new(),
            current_model: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl SttAdapter for WhisperAdapter {
    async fn initialize(&mut self, config: SttConfig) -> Result<()> {
        let model_name = config.model_name.clone();
        if is_sherpa_model_name(&model_name) {
            self.sherpa.initialize(config).await?;
        } else {
            self.whisper.initialize(config).await?;
        }

        if let Ok(mut slot) = self.current_model.lock() {
            *slot = Some(model_name);
        }
        Ok(())
    }

    async fn transcribe(&self, audio_data: &[f32], format: AudioFormat) -> Result<Transcription> {
        let model_name = self
            .current_model
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| SttError::TranscriptionFailed("adapter not initialized".into()))?;

        if is_sherpa_model_name(&model_name) {
            self.sherpa.transcribe(audio_data, format).await
        } else {
            self.whisper.transcribe(audio_data, format).await
        }
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        if is_sherpa_model_name(model_name) {
            self.sherpa.is_model_available(model_name).await
        } else {
            self.whisper.is_model_available(model_name).await
        }
    }

    fn available_models(&self) -> Vec<String> {
        self.whisper.available_models()
    }

    fn current_model(&self) -> Option<String> {
        self.current_model.lock().ok().and_then(|slot| slot.clone())
    }
}

impl Default for WhisperAdapter {
    fn default() -> Self {
        Self::new()
    }
}
