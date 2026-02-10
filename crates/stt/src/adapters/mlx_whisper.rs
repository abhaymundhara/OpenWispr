use crate::{
    emit_model_download_progress, is_mlx_whisper_model_name, AudioFormat, ModelDownloadProgress,
    Result, SttConfig, SttError, TranscriptSegment, Transcription,
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

pub(crate) struct SharedMlxWhisperAdapter {
    state: Arc<RwLock<MlxState>>,
}

impl SharedMlxWhisperAdapter {
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
            message: Some("Preparing MLX Whisper runtime".to_string()),
        });

        tokio::task::spawn_blocking({
            let model_ref = model_ref.clone();
            let cache_dir = cache_dir.clone();
            move || ensure_whisper_ready(&model_ref, &cache_dir)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("mlx whisper setup task failed: {e}")))??;
        
        // Marker file creation to signal readiness
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
            message: Some("MLX Whisper model ready".to_string()),
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
                .ok_or_else(|| SttError::TranscriptionFailed("mlx whisper adapter not initialized".into()))?
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
        .map_err(|e| SttError::TranscriptionFailed(format!("mlx whisper decode task failed: {e}")))??;

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
            language: Some("hi".to_string()), // Default/Guessing Hindi for this specific model, or generic
            confidence: None,
            segments,
        })
    }

    pub(crate) async fn is_model_available(&self, model_name: &str) -> bool {
        if !is_mlx_whisper_model_name(model_name) {
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

fn ensure_whisper_ready(model_ref: &str, cache_dir: &Path) -> Result<()> {
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
        message: Some("Preparing mlx-whisper package".to_string()),
    });
    ensure_whisper_package_installed(cache_dir)?;

    let script = r#"
import sys
import os
import json
import shutil
from pathlib import Path
import mlx.core as mx

# Ensure imports
try:
    import mlx_whisper
    from huggingface_hub import snapshot_download
except ImportError:
    print("Missing required packages")
    sys.exit(1)

model_ref = sys.argv[1]
# We use a subdirectory for the converted model to avoid pollution/conflict
# asking huggingface_hub where it stores is good, but we want a known path for converted weights
# We will download snapshot to default HF cache, but assume we can read it.
# Actually, snapshot_download returns the path.

print(f"Ensuring model {model_ref} is available...")
try:
    # prompt download
    model_path = snapshot_download(repo_id=model_ref)
    print(f"Model downloaded to {model_path}")
except Exception as e:
    print(f"Error downloading model: {e}")
    sys.exit(1)

# Check if we need to convert/patch
# We define a 'ready' marker or just check for weights.npz and clean config
# Custom conversion path inside the HF cache might be tricky as it is read-only usually?
# No, HF cache is user cache. But structure is managed by HF.
# We should copy/convert to our OWN cache dir for the final ready model.
override_cache_dir = sys.argv[2]
dest_dir = Path(override_cache_dir) / "converted_" / model_ref.replace("/", "_")
dest_dir.mkdir(parents=True, exist_ok=True)

config_dest = dest_dir / "config.json"
weights_dest = dest_dir / "weights.npz"

if config_dest.exists() and weights_dest.exists():
    print("Converted model already exists.")
else:
    print("Converting/Patching model...")
    # 1. Config Patching
    with open(Path(model_path) / "config.json", "r") as f:
        hf_config = json.load(f)

    # Mapping HF keys to ModelDimensions keys for mlx-whisper
    # Based on OpenAI Whisper dimensions
    mapping = {
        "num_mel_bins": "n_mels",
        "max_source_positions": "n_audio_ctx",
        "d_model": "n_audio_state",
        "encoder_attention_heads": "n_audio_head",
        "encoder_layers": "n_audio_layer",
        "vocab_size": "n_vocab",
        "max_target_positions": "n_text_ctx",
        "decoder_attention_heads": "n_text_head",
        "decoder_layers": "n_text_layer",
        # n_text_state is usually d_model too
    }

    new_config = {}
    for hf_key, mlx_key in mapping.items():
        if hf_key in hf_config:
            new_config[mlx_key] = hf_config[hf_key]
    
    if "d_model" in hf_config:
        new_config["n_text_state"] = hf_config["d_model"]

    with open(config_dest, "w") as f:
        json.dump(new_config, f, indent=2)
    print("Config patched.")

    # 2. Weights Conversion
    # We load safetensors and map keys
    if (Path(model_path) / "model.safetensors").exists():
        print("Loading safetensors...")
        weights = mx.load(str(Path(model_path) / "model.safetensors"))
        new_weights = {}
        for k, v in weights.items():
            new_key = k
            # Strip model. prefix
            if new_key.startswith("model."):
                new_key = new_key[6:]
            
            # Global mappings
            if new_key == "encoder.layer_norm.weight": new_key = "encoder.ln_post.weight"
            elif new_key == "encoder.layer_norm.bias": new_key = "encoder.ln_post.bias"
            elif new_key == "decoder.layer_norm.weight": new_key = "decoder.ln.weight"
            elif new_key == "decoder.layer_norm.bias": new_key = "decoder.ln.bias"
            elif new_key == "decoder.embed_tokens.weight": new_key = "decoder.token_embedding.weight"
            # dec positional
            elif new_key == "decoder.embed_positions.weight": new_key = "decoder.positional_embedding"
            
            # Block mappings
            # encoder.layers.X -> encoder.blocks.X
            if "layers." in new_key:
                new_key = new_key.replace("layers.", "blocks.")
            
            # Sub-block mappings
            if ".fc1." in new_key: new_key = new_key.replace(".fc1.", ".mlp1.")
            if ".fc2." in new_key: new_key = new_key.replace(".fc2.", ".mlp2.")
            if ".final_layer_norm." in new_key: new_key = new_key.replace(".final_layer_norm.", ".mlp_ln.")
            if ".self_attn_layer_norm." in new_key: new_key = new_key.replace(".self_attn_layer_norm.", ".attn_ln.")
            
            if ".self_attn.q_proj." in new_key: new_key = new_key.replace(".self_attn.q_proj.", ".attn.query.")
            if ".self_attn.k_proj." in new_key: new_key = new_key.replace(".self_attn.k_proj.", ".attn.key.")
            if ".self_attn.v_proj." in new_key: new_key = new_key.replace(".self_attn.v_proj.", ".attn.value.")
            if ".self_attn.out_proj." in new_key: new_key = new_key.replace(".self_attn.out_proj.", ".attn.out.")
            
            # Filter out encoder positions if MLX doesn't usually use them (sinusoidal)
            # HF: encoder.embed_positions.weight
            if new_key == "encoder.embed_positions.weight":
                continue 

            new_weights[new_key] = v
            
        print("Saving converted weights...")
        mx.savez(str(weights_dest), **new_weights)
    else:
        print("model.safetensors not found!")
        sys.exit(1)

print("Setup complete.")
print(f"READY_PATH={dest_dir}")
"#;
    
    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "download".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(40.0),
        done: false,
        error: None,
        message: Some("Downloading and converting MLX Whisper model".to_string()),
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
            "failed to download/convert MLX model '{}': {}",
            model_ref,
            compact_python_error(&output.stderr)
        );
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_ref.to_string(),
            stage: "download".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(0.0),
            done: true,
            error: Some(message.clone()),
            message: Some("MLX Whisper model setup failed".to_string()),
        });
        return Err(SttError::ModelLoadError(message));
    }

    // Capture the destination path from stdout to use it for future calls if needed, 
    // but better to just deterministically know it.
    // The script prints READY_PATH=...
    // We can rely on the deterministic path: cache_dir/converted/SANITIZED_REF
    
    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_ref.to_string(),
        stage: "download".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: Some(100.0),
        done: true,
        error: None,
        message: Some("MLX Whisper model ready".to_string()),
    });

    Ok(())
}

fn run_mlx_transcription(model_ref: &str, cache_dir: &Path, wav_path: &Path) -> Result<String> {
    let converted_path = converted_model_path(model_ref)?;
    let script = r#"
import sys
import mlx_whisper

model_path = sys.argv[1]
wav_path = sys.argv[2]

# Ensure we use the converted model path
result = mlx_whisper.transcribe(wav_path, path_or_hf_repo=model_path)
print(result["text"])
"#;

    let python_bin = venv_python_bin(cache_dir);
    let output = Command::new(&python_bin)
        .args([
            "-c",
            script,
            &converted_path.to_string_lossy(),
            &wav_path.to_string_lossy(),
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
    let text = stdout.trim().to_string();

    Ok(text)
}

fn ensure_python_available() -> Result<()> {
    let output = Command::new(PYTHON_BIN)
        .arg("--version")
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "Python is required for MLX runtime ({PYTHON_BIN} not found): {e}"
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }

    Err(SttError::ModelLoadError(
        "Python is required for MLX runtime but --version failed".into(),
    ))
}

fn ensure_whisper_package_installed(cache_dir: &Path) -> Result<()> {
    let python_bin = ensure_venv_ready(cache_dir)?;
    let check = Command::new(&python_bin)
        .args(["-c", "import mlx_whisper"])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to probe mlx-whisper in MLX runtime ({}): {e}",
                python_bin.display()
            ))
        })?;
    if check.status.success() {
        return Ok(());
    }

    let install = Command::new(&python_bin)
        .args(["-m", "pip", "install", "--upgrade", "mlx-whisper", "huggingface_hub"])
        .output()
        .map_err(|e| {
            SttError::ModelLoadError(format!(
                "failed to install mlx-whisper in MLX runtime ({}): {e}",
                python_bin.display()
            ))
        })?;
    if install.status.success() {
        return Ok(());
    }

    Err(SttError::ModelLoadError(format!(
        "failed to install mlx-whisper: {}",
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

fn converted_model_path(model_ref: &str) -> Result<PathBuf> {
    Ok(mlx_cache_dir()?
        .join(format!("converted_{}", model_ref.replace('/', "_"))))
}

fn marker_file_path(model_ref: &str) -> Result<PathBuf> {
    Ok(converted_model_path(model_ref)?.join("ready"))
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
        "openwispr-mlx-whisper-{}-{}.wav",
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
