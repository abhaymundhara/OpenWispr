//! macOS STT adapter.
//! Routes between whisper.cpp, Sherpa ONNX and MLX Parakeet based on selected model.

use crate::{
    is_mlx_model_name, is_sherpa_model_name, AudioFormat, Result, SttAdapter, SttConfig,
    Transcription,
};
use async_trait::async_trait;
use tracing::{info, warn};

use super::backend::SharedWhisperAdapter;
use super::mlx_parakeet::SharedMlxParakeetAdapter;
use super::sherpa::SharedSherpaAdapter;
use std::sync::{Arc, Mutex};

pub struct MlxAdapter {
    whisper: SharedWhisperAdapter,
    sherpa: SharedSherpaAdapter,
    mlx_parakeet: SharedMlxParakeetAdapter,
    current_model: Arc<Mutex<Option<String>>>,
}

impl MlxAdapter {
    pub fn new() -> Self {
        info!("Initializing macOS STT adapter");
        Self {
            whisper: SharedWhisperAdapter::new("macOS whisper backend"),
            sherpa: SharedSherpaAdapter::new(),
            mlx_parakeet: SharedMlxParakeetAdapter::new(),
            current_model: Arc::new(Mutex::new(None)),
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
        let model_name = config.model_name.clone();
        if is_sherpa_model_name(&model_name) {
            self.sherpa.initialize(config).await?;
        } else if is_mlx_model_name(&model_name) {
            self.mlx_parakeet.initialize(config).await?;
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
            .ok_or_else(|| crate::SttError::TranscriptionFailed("adapter not initialized".into()))?;

        if is_sherpa_model_name(&model_name) {
            self.sherpa.transcribe(audio_data, format).await
        } else if is_mlx_model_name(&model_name) {
            self.mlx_parakeet.transcribe(audio_data, format).await
        } else {
            self.whisper.transcribe(audio_data, format).await
        }
    }

    async fn is_model_available(&self, model_name: &str) -> bool {
        if is_sherpa_model_name(model_name) {
            self.sherpa.is_model_available(model_name).await
        } else if is_mlx_model_name(model_name) {
            self.mlx_parakeet.is_model_available(model_name).await
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

impl Default for MlxAdapter {
    fn default() -> Self {
        Self::new()
    }
}
