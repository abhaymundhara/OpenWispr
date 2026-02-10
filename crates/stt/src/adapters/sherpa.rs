use crate::{
    emit_model_download_progress, is_sherpa_model_name, AudioFormat, ModelDownloadProgress, Result,
    SttConfig, SttError, TranscriptSegment, Transcription,
};
use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use super::backend::{prepare_audio, TARGET_SAMPLE_RATE};

const SHERPA_PARKEET_BASE_URL: &str =
    "https://huggingface.co/nasedkinpv/parakeet-tdt-0.6b-v3-onnx-int8/resolve/main";
const SHERPA_PARKEET_RELEASE_DIR: &str = "parakeet-tdt-0.6b-v3-onnx-int8";

// Map: Local Filename -> Remote Filename
const SHERPA_FILES: &[(&str, &str)] = &[
    ("encoder.int8.onnx", "encoder-int8.onnx"),
    ("encoder.int8.onnx.data", "encoder-int8.onnx.data"),
    ("decoder.int8.onnx", "decoder_joint-int8.onnx"),
    ("tokens.txt", "vocab.txt"),
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

}

fn create_recognizer(model_root: &Path) -> Result<TransducerRecognizer> {
    // Note: We use decoder.int8.onnx (which is actually decoder_joint-int8.onnx)
    // for both decoder and joiner as a fallback, hoping the joiner graph is compatible 
    // or contained within. If this fails, we need to revisit the model structure.
    // Standard Sherpa expects separate files, but some ONNX exports combine them.
    let cfg = TransducerConfig {
        encoder: model_root
            .join("encoder.int8.onnx")
            .to_string_lossy()
            .to_string(),
        decoder: model_root
            .join("decoder.int8.onnx")
            .to_string_lossy()
            .to_string(),
        joiner: model_root
            .join("decoder.int8.onnx") // Use joint file for joiner as well
            .to_string_lossy()
            .to_string(),
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
    let model_name = "sherpa-onnx/parakeet-tdt-0.6b-v3-int8".to_string();
    let root = sherpa_model_root_dir()?;
    if has_required_files(&root) {
        emit_model_download_progress(ModelDownloadProgress {
            model_name,
            stage: "ready".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(100.0),
            done: true,
            error: None,
            message: Some("Model already downloaded".to_string()),
        });
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

    let download_dir = cache_dir.join(format!("{SHERPA_PARKEET_RELEASE_DIR}.download"));
    if download_dir.exists() {
        let _ = fs::remove_dir_all(&download_dir);
    }
    fs::create_dir_all(&download_dir).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create temporary download directory {}: {e}",
            download_dir.display()
        ))
    })?;

    // Download each file
    let total_files = SHERPA_FILES.len();
    for (idx, (local_name, remote_name)) in SHERPA_FILES.iter().enumerate() {
        let url = format!("{}/{}", SHERPA_PARKEET_BASE_URL, remote_name);
        let dest_path = download_dir.join(local_name);
        
        let message = format!("Downloading file {}/{} ({})", idx + 1, total_files, local_name);
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_name.clone(),
            stage: "download".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some((idx as f32 / total_files as f32) * 100.0),
            done: false,
            error: None,
            message: Some(message.clone()),
        });

        download_file(&url, &dest_path, &model_name, idx, total_files)?;
    }

    // Move to final location
    fs::rename(&download_dir, &root).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to move downloaded model to {}: {e}",
            root.display()
        ))
    })?;

    emit_model_download_progress(ModelDownloadProgress {
        model_name,
        stage: "ready".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(100.0),
        done: true,
        error: None,
        message: Some("Sherpa model ready".to_string()),
    });

    Ok(root)
}

fn download_file(
    url: &str, 
    dest: &Path, 
    model_name: &str, 
    file_idx: usize, 
    total_files: usize
) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| {
            SttError::ModelLoadError(format!("failed to download {}: {e}", url))
        })?;

    let total_bytes = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);

    let mut reader = response.into_reader();
    let mut writer = BufWriter::new(File::create(dest).map_err(|e| {
        SttError::ModelLoadError(format!("failed to create file {}: {e}", dest.display()))
    })?);

    let mut downloaded_bytes = 0_u64;
    let mut last_emitted = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let n = reader.read(&mut buffer).map_err(|e| {
            SttError::ModelLoadError(format!("read error: {e}"))
        })?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n]).map_err(|e| {
            SttError::ModelLoadError(format!("write error: {e}"))
        })?;

        downloaded_bytes += n as u64;
        if downloaded_bytes.saturating_sub(last_emitted) >= 1024 * 1024 
            || total_bytes.is_some_and(|t| downloaded_bytes >= t) 
        {
            let file_percent = total_bytes.map(|t| downloaded_bytes as f32 / t as f32);
            let total_percent = if let Some(p) = file_percent {
                ((file_idx as f32 + p) / total_files as f32) * 100.0
            } else {
                (file_idx as f32 / total_files as f32) * 100.0
            };

            emit_model_download_progress(ModelDownloadProgress {
                model_name: model_name.to_string(),
                stage: "download".to_string(),
                downloaded_bytes,
                total_bytes,
                percent: Some(total_percent),
                done: false,
                error: None,
                message: Some(format!("Downloading {}", dest.file_name().unwrap_or_default().to_string_lossy())),
            });
            last_emitted = downloaded_bytes;
        }
    }
    writer.flush().map_err(|e| SttError::ModelLoadError(format!("flush error: {e}")))?;
    Ok(())
}

fn has_required_files(root: &Path) -> bool {
    root.is_dir()
        && SHERPA_FILES
            .iter()
            .all(|(local_name, _)| root.join(local_name).exists())
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
