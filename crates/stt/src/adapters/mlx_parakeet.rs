use crate::{
    emit_model_download_progress, is_mlx_model_name, AudioFormat, ModelDownloadProgress, Result,
    SttConfig, SttError, TranscriptSegment, Transcription,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::RwLock;

use std::sync::Arc;

use super::backend::{prepare_audio, TARGET_SAMPLE_RATE};

const PYTHON_BIN: &str = "python3";
const MLX_VENV_DIR: &str = ".venv";

#[derive(Default)]
struct MlxState {
    config: Option<SttConfig>,
    model_ref: Option<String>,
}

pub(crate) struct SharedMlxParakeetAdapter {
    state: Arc<RwLock<MlxState>>,
}

impl SharedMlxParakeetAdapter {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(MlxState::default())),
        }
    }

    pub(crate) async fn initialize(&self, config: SttConfig) -> Result<()> {
        let model_ref = resolve_model_ref(&config)?;
        let cache_dir = mlx_cache_dir()?;

        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_ref.clone(),
            stage: "prepare".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(0.0),
            done: false,
            error: None,
            message: Some("Preparing MLX runtime".to_string()),
        });

        tokio::task::spawn_blocking({
            let model_ref = model_ref.clone();
            let cache_dir = cache_dir.clone();
            move || ensure_parakeet_ready(&model_ref, &cache_dir)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("mlx setup task failed: {e}")))??;

        let marker = marker_file_path(&model_ref)?;
        if let Some(parent) = marker.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SttError::ModelLoadError(format!(
                    "failed to create mlx marker directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
        fs::write(&marker, b"ready").map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to write mlx marker {}: {e}",
                marker.display()
            ))
        })?;
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_ref.clone(),
            stage: "ready".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(100.0),
            done: true,
            error: None,
            message: Some("MLX model ready".to_string()),
        });

        let mut state = self.state.write().await;
        state.model_ref = Some(model_ref);
        state.config = Some(config);
        Ok(())
    }

    pub(crate) async fn transcribe(
        &self,
        audio_data: &[f32],
        format: AudioFormat,
    ) -> Result<Transcription> {
        let model_ref = {
            let state = self.state.read().await;
            state
                .model_ref
                .clone()
                .ok_or_else(|| SttError::TranscriptionFailed("mlx adapter not initialized".into()))?
        };

        let prepared = prepare_audio(audio_data, &format);
        if prepared.is_empty() {
            return Err(SttError::AudioError(
                "no audio samples available after preprocessing".into(),
            ));
        }

        let duration_s = prepared.len() as f64 / TARGET_SAMPLE_RATE as f64;
        let cache_dir = mlx_cache_dir()?;
        let text = tokio::task::spawn_blocking(move || {
            let temp_wav = temp_wav_path();
            write_mono_wav(&temp_wav, &prepared, TARGET_SAMPLE_RATE)?;
            let result = run_mlx_transcription(&model_ref, &cache_dir, &temp_wav);
            let _ = fs::remove_file(&temp_wav);
            result
        })
        .await
        .map_err(|e| SttError::TranscriptionFailed(format!("mlx decode task failed: {e}")))??;

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
        if !is_mlx_model_name(model_name) {
            return false;
        }
        marker_file_path(model_name)
            .map(|path| path.exists())
            .unwrap_or(false)
    }

}

fn resolve_model_ref(config: &SttConfig) -> Result<String> {
    if let Some(path) = config.model_path.clone() {
        return Ok(path.to_string_lossy().to_string());
    }

    if config.model_name.contains('/') {
        return Ok(config.model_name.clone());
    }

    Err(SttError::ModelNotFound(format!(
        "unsupported MLX model '{}', expected a Hugging Face repo id",
        config.model_name
    )))
}

fn ensure_parakeet_ready(model_ref: &str, cache_dir: &Path) -> Result<()> {
    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "runtime-check".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(10.0),
        done: false,
        error: None,
        message: Some("Checking Python runtime".to_string()),
    });
    ensure_python_available()?;

    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "runtime-check".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(25.0),
        done: false,
        error: None,
        message: Some("Preparing parakeet-mlx package".to_string()),
    });
    ensure_parakeet_package_installed(cache_dir)?;

    let script = r#"
import sys
from parakeet_mlx import from_pretrained
model_ref = sys.argv[1]
cache_dir = sys.argv[2]
from_pretrained(model_ref, cache_dir=cache_dir)
"#;
    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "download".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(40.0),
        done: false,
        error: None,
        message: Some("Downloading MLX model weights".to_string()),
    });

    let python_bin = venv_python_bin(cache_dir);
    let output = Command::new(&python_bin)
        .args(["-c", script, model_ref, &cache_dir.to_string_lossy()])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to start MLX Python runtime ({}): {e}",
                python_bin.display()
            ))
        })?;

    if !output.status.success() {
        let message = format!(
            "failed to download/load MLX model '{}': {}",
            model_ref,
            compact_python_error(&output.stderr)
        );
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_ref.to_string(),
            stage: "download".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(40.0),
            done: true,
            error: Some(message.clone()),
            message: Some("MLX model setup failed".to_string()),
        });
        return Err(SttError::ModelLoadError(message));
    }
    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "download".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(90.0),
        done: false,
        error: None,
        message: Some("Finalizing MLX model".to_string()),
    });

    Ok(())
}

fn run_mlx_transcription(model_ref: &str, cache_dir: &Path, wav_path: &Path) -> Result<String> {
    let script = r#"
import sys
from parakeet_mlx import from_pretrained

model_ref = sys.argv[1]
wav_path = sys.argv[2]
cache_dir = sys.argv[3]

model = from_pretrained(model_ref, cache_dir=cache_dir)
result = model.transcribe(wav_path)

text = getattr(result, "text", None)
if text is None and isinstance(result, dict):
    text = result.get("text")
if text is None and hasattr(result, "__dict__"):
    text = result.__dict__.get("text")
if text is None:
    text = str(result)
print((text or "").strip())
"#;

    let python_bin = venv_python_bin(cache_dir);
    let output = Command::new(&python_bin)
        .args([
            "-c",
            script,
            model_ref,
            &wav_path.to_string_lossy(),
            &cache_dir.to_string_lossy(),
        ])
        .output()
        .map_err(|e| {
            SttError::TranscriptionFailed(format!(
                "failed to start MLX Python runtime ({}): {e}",
                python_bin.display()
            ))
        })?;

    if !output.status.success() {
        return Err(SttError::TranscriptionFailed(format!(
            "MLX transcription failed: {}",
            compact_python_error(&output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let text = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .unwrap_or("")
        .to_string();

    Ok(text)
}

fn ensure_python_available() -> Result<()> {
    let output = Command::new(PYTHON_BIN)
        .arg("--version")
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "Python is required for MLX Parakeet runtime ({PYTHON_BIN} not found): {e}"
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }

    Err(SttError::ModelLoadError(
        "Python is required for MLX Parakeet runtime but --version failed".into(),
    ))
}

fn ensure_parakeet_package_installed(cache_dir: &Path) -> Result<()> {
    let python_bin = ensure_venv_ready(cache_dir)?;
    let check = Command::new(&python_bin)
        .args(["-c", "import parakeet_mlx"])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to probe parakeet-mlx in MLX runtime ({}): {e}",
                python_bin.display()
            ))
        })?;
    if check.status.success() {
        return Ok(());
    }

    let install = Command::new(&python_bin)
        .args(["-m", "pip", "install", "--upgrade", "parakeet-mlx"])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to install parakeet-mlx in MLX runtime ({}): {e}",
                python_bin.display()
            ))
        })?;
    if install.status.success() {
        return Ok(());
    }

    Err(SttError::ModelLoadError(format!(
        "failed to install parakeet-mlx: {}",
        compact_python_error(&install.stderr)
    )))
}

fn ensure_venv_ready(cache_dir: &Path) -> Result<PathBuf> {
    let python_bin = venv_python_bin(cache_dir);
    if python_bin.exists() {
        return Ok(python_bin);
    }

    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir).map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to create MLX cache directory {}: {e}",
                cache_dir.display()
            ))
        })?;
    }

    let venv_dir = cache_dir.join(MLX_VENV_DIR);
    let create = Command::new(PYTHON_BIN)
        .args(["-m", "venv", &venv_dir.to_string_lossy()])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to create MLX virtualenv with {PYTHON_BIN}: {e}"
            ))
        })?;

    if !create.status.success() {
        return Err(SttError::ModelLoadError(format!(
            "failed to create MLX virtualenv: {}",
            compact_python_error(&create.stderr)
        )));
    }

    if !python_bin.exists() {
        return Err(SttError::ModelLoadError(format!(
            "MLX virtualenv created but interpreter missing at {}",
            python_bin.display()
        )));
    }

    Ok(python_bin)
}

fn venv_python_bin(cache_dir: &Path) -> PathBuf {
    cache_dir.join(MLX_VENV_DIR).join("bin").join("python3")
}

fn compact_python_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("unknown Python error");

    const MAX_LEN: usize = 220;
    if first_line.chars().count() > MAX_LEN {
        let clipped: String = first_line.chars().take(MAX_LEN).collect();
        format!("{clipped}...")
    } else {
        first_line.to_string()
    }
}

fn marker_file_path(model_ref: &str) -> Result<PathBuf> {
    Ok(mlx_cache_dir()?
        .join(".downloaded")
        .join(sanitize_model_ref(model_ref))
        .join("ready"))
}

fn sanitize_model_ref(model_ref: &str) -> String {
    model_ref
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn mlx_cache_dir() -> Result<PathBuf> {
    Ok(base_model_cache_dir()?.join("mlx"))
}

fn base_model_cache_dir() -> Result<PathBuf> {
    if let Ok(override_dir) = std::env::var("OPENWISPR_MODEL_DIR") {
        if !override_dir.trim().is_empty() {
            return Ok(PathBuf::from(override_dir));
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home)
                .join(".cache")
                .join("openwispr")
                .join("models"));
        }
    }

    Err(SttError::ModelLoadError(
        "unable to determine model cache directory".into(),
    ))
}

fn temp_wav_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "openwispr-mlx-{}-{}.wav",
        std::process::id(),
        stamp
    ))
}

fn write_mono_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| {
        SttError::AudioError(format!("failed to create temporary wav {}: {e}", path.display()))
    })?;

    for sample in samples {
        let scaled = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        writer.write_sample(scaled).map_err(|e| {
            SttError::AudioError(format!("failed to write temporary wav {}: {e}", path.display()))
        })?;
    }

    writer.finalize().map_err(|e| {
        SttError::AudioError(format!(
            "failed to finalize temporary wav {}: {e}",
            path.display()
        ))
    })?;
    Ok(())
}
