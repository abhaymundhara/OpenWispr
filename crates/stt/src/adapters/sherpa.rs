use crate::{
    is_sherpa_model_name, AudioFormat, Result, SttConfig, SttError, TranscriptSegment,
    Transcription, SHERPA_PARAKEET_INT8_MODEL,
};
use bzip2::read::BzDecoder;
use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use super::backend::{prepare_audio, TARGET_SAMPLE_RATE};

const SHERPA_PARKEET_RELEASE_ARCHIVE: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2";
const SHERPA_PARKEET_RELEASE_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8";
const SHERPA_PARKEET_RELEASE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2";
const SHERPA_REQUIRED_FILES: &[&str] = &[
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

#[derive(Default)]
struct SherpaState {
    config: Option<SttConfig>,
    model_root: Option<PathBuf>,
    recognizer: Option<Arc<Mutex<TransducerRecognizer>>>,
}

pub(crate) struct SharedSherpaAdapter {
    state: Arc<RwLock<SherpaState>>,
}

impl SharedSherpaAdapter {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SherpaState::default())),
        }
    }

    pub(crate) async fn initialize(&self, config: SttConfig) -> Result<()> {
        let model_root = tokio::task::spawn_blocking({
            let config = config.clone();
            move || resolve_model_root(&config)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("sherpa model path task failed: {e}")))??;

        let recognizer = tokio::task::spawn_blocking({
            let model_root = model_root.clone();
            move || create_recognizer(&model_root)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("sherpa init task failed: {e}")))??;

        let mut state = self.state.write().await;
        state.config = Some(config);
        state.model_root = Some(model_root);
        state.recognizer = Some(Arc::new(Mutex::new(recognizer)));
        Ok(())
    }

    pub(crate) async fn transcribe(
        &self,
        audio_data: &[f32],
        format: AudioFormat,
    ) -> Result<Transcription> {
        let recognizer = {
            let state = self.state.read().await;
            state
                .recognizer
                .clone()
                .ok_or_else(|| SttError::TranscriptionFailed("sherpa adapter not initialized".into()))?
        };

        let prepared = prepare_audio(audio_data, &format);
        if prepared.is_empty() {
            return Err(SttError::AudioError(
                "no audio samples available after preprocessing".into(),
            ));
        }

        let duration_s = prepared.len() as f64 / TARGET_SAMPLE_RATE as f64;
        let text = tokio::task::spawn_blocking(move || {
            let mut guard = recognizer.lock().map_err(|_| {
                SttError::TranscriptionFailed("failed to lock sherpa recognizer".into())
            })?;
            Ok::<String, SttError>(guard.transcribe(TARGET_SAMPLE_RATE, &prepared))
        })
        .await
        .map_err(|e| SttError::TranscriptionFailed(format!("sherpa decode task failed: {e}")))??;

        let clean = text.trim().to_string();
        let mut segments = Vec::new();
        if !clean.is_empty() {
            segments.push(TranscriptSegment {
                text: clean.clone(),
                start: 0.0,
                end: duration_s,
            });
        }

        Ok(Transcription {
            text: clean,
            language: Some("en".to_string()),
            confidence: None,
            segments,
        })
    }

    pub(crate) async fn is_model_available(&self, model_name: &str) -> bool {
        if looks_like_model_dir(model_name) {
            return has_required_files(Path::new(model_name));
        }
        if !is_sherpa_model_name(model_name) {
            return false;
        }
        sherpa_model_root_dir()
            .map(|root| has_required_files(&root))
            .unwrap_or(false)
    }

    pub(crate) fn available_models(&self) -> Vec<String> {
        vec![SHERPA_PARAKEET_INT8_MODEL.to_string()]
    }

    pub(crate) fn current_model(&self) -> Option<String> {
        self.state
            .blocking_read()
            .config
            .as_ref()
            .map(|cfg| cfg.model_name.clone())
    }
}

fn create_recognizer(model_root: &Path) -> Result<TransducerRecognizer> {
    let cfg = TransducerConfig {
        encoder: model_root
            .join("encoder.int8.onnx")
            .to_string_lossy()
            .to_string(),
        decoder: model_root
            .join("decoder.int8.onnx")
            .to_string_lossy()
            .to_string(),
        joiner: model_root.join("joiner.int8.onnx").to_string_lossy().to_string(),
        tokens: model_root.join("tokens.txt").to_string_lossy().to_string(),
        model_type: "nemo_transducer".to_string(),
        decoding_method: "greedy_search".to_string(),
        sample_rate: TARGET_SAMPLE_RATE as i32,
        feature_dim: 80,
        num_threads: optimal_threads(),
        provider: Some("cpu".to_string()),
        ..Default::default()
    };

    TransducerRecognizer::new(cfg).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to initialize sherpa recognizer from {}: {e}",
            model_root.display()
        ))
    })
}

fn resolve_model_root(config: &SttConfig) -> Result<PathBuf> {
    if let Some(path) = config.model_path.clone() {
        if has_required_files(&path) {
            return Ok(path);
        }
        return Err(SttError::ModelNotFound(format!(
            "sherpa model files not found at {}",
            path.display()
        )));
    }

    if looks_like_model_dir(&config.model_name) {
        let path = PathBuf::from(&config.model_name);
        if has_required_files(&path) {
            return Ok(path);
        }
        return Err(SttError::ModelNotFound(format!(
            "sherpa model files not found at {}",
            path.display()
        )));
    }

    if !is_sherpa_model_name(&config.model_name) {
        return Err(SttError::ModelNotFound(format!(
            "unsupported sherpa model: {}",
            config.model_name
        )));
    }

    ensure_model_downloaded()
}

fn ensure_model_downloaded() -> Result<PathBuf> {
    let root = sherpa_model_root_dir()?;
    if has_required_files(&root) {
        return Ok(root);
    }

    if root.exists() {
        let _ = fs::remove_dir_all(&root);
    }

    let cache_dir = sherpa_cache_dir()?;
    fs::create_dir_all(&cache_dir).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create sherpa model cache directory {}: {e}",
            cache_dir.display()
        ))
    })?;

    let archive_tmp = cache_dir.join(format!("{SHERPA_PARKEET_RELEASE_ARCHIVE}.download"));
    let response = ureq::get(SHERPA_PARKEET_RELEASE_URL)
        .call()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to download sherpa model {}: {e}",
                SHERPA_PARKEET_RELEASE_URL
            ))
        })?;

    let mut reader = response.into_reader();
    let mut writer = BufWriter::new(File::create(&archive_tmp).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create temporary archive {}: {e}",
            archive_tmp.display()
        ))
    })?);
    io::copy(&mut reader, &mut writer).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed while writing sherpa archive {}: {e}",
            archive_tmp.display()
        ))
    })?;
    writer.flush().map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to flush sherpa archive {}: {e}",
            archive_tmp.display()
        ))
    })?;

    let unpack_dir = cache_dir.join(format!("{SHERPA_PARKEET_RELEASE_DIR}.unpack"));
    if unpack_dir.exists() {
        let _ = fs::remove_dir_all(&unpack_dir);
    }
    fs::create_dir_all(&unpack_dir).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create sherpa unpack directory {}: {e}",
            unpack_dir.display()
        ))
    })?;

    let archive_file = File::open(&archive_tmp).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to open downloaded archive {}: {e}",
            archive_tmp.display()
        ))
    })?;
    let decoder = BzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&unpack_dir).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to extract sherpa archive {}: {e}",
            archive_tmp.display()
        ))
    })?;

    let extracted_root = unpack_dir.join(SHERPA_PARKEET_RELEASE_DIR);
    let source_dir = if extracted_root.exists() {
        extracted_root
    } else {
        fs::read_dir(&unpack_dir)
            .map_err(|e| {
                SttError::ModelLoadError(format!(
                    "failed to inspect extracted sherpa model directory {}: {e}",
                    unpack_dir.display()
                ))
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.is_dir())
            .ok_or_else(|| {
                SttError::ModelLoadError(format!(
                    "could not find extracted sherpa model directory in {}",
                    unpack_dir.display()
                ))
            })?
    };

    fs::rename(&source_dir, &root).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to move sherpa model to {}: {e}",
            root.display()
        ))
    })?;

    let _ = fs::remove_dir_all(&unpack_dir);
    let _ = fs::remove_file(&archive_tmp);

    if !has_required_files(&root) {
        return Err(SttError::ModelLoadError(format!(
            "downloaded sherpa model is missing required files at {}",
            root.display()
        )));
    }

    Ok(root)
}

fn has_required_files(root: &Path) -> bool {
    root.is_dir()
        && SHERPA_REQUIRED_FILES
            .iter()
            .all(|name| root.join(name).exists())
}

fn sherpa_cache_dir() -> Result<PathBuf> {
    Ok(base_model_cache_dir()?.join("sherpa-onnx"))
}

fn sherpa_model_root_dir() -> Result<PathBuf> {
    Ok(sherpa_cache_dir()?.join(SHERPA_PARKEET_RELEASE_DIR))
}

fn base_model_cache_dir() -> Result<PathBuf> {
    if let Ok(override_dir) = std::env::var("OPENWISPR_MODEL_DIR") {
        if !override_dir.trim().is_empty() {
            return Ok(PathBuf::from(override_dir));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            if !local_app_data.trim().is_empty() {
                return Ok(PathBuf::from(local_app_data)
                    .join("OpenWispr")
                    .join("models"));
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return Ok(PathBuf::from(home)
                    .join(".cache")
                    .join("openwispr")
                    .join("models"));
            }
        }
    }

    Err(SttError::ModelLoadError(
        "unable to determine model cache directory".into(),
    ))
}

fn looks_like_model_dir(model_name: &str) -> bool {
    model_name.contains('/')
        || model_name.contains('\\')
        || model_name.ends_with(".onnx")
        || model_name.ends_with(".txt")
}

fn optimal_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4) as i32
}
