use crate::{
    AudioFormat, Result, SttConfig, SttError, TranscriptSegment, Transcription, TranscriptionTask,
};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use whisper_rs::{
    get_lang_str, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

pub(crate) const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Default)]
struct SharedState {
    config: Option<SttConfig>,
    model_path: Option<PathBuf>,
    context: Option<Arc<WhisperContext>>,
}

/// Shared whisper.cpp backend used by both macOS and Windows adapters.
pub(crate) struct SharedWhisperAdapter {
    runtime_name: &'static str,
    state: Arc<RwLock<SharedState>>,
}

impl SharedWhisperAdapter {
    pub(crate) fn new(runtime_name: &'static str) -> Self {
        Self {
            runtime_name,
            state: Arc::new(RwLock::new(SharedState::default())),
        }
    }

    pub(crate) async fn initialize(&self, config: SttConfig) -> Result<()> {
        let runtime_name = self.runtime_name;
        let model_path = tokio::task::spawn_blocking({
            let config = config.clone();
            move || resolve_model_path(&config)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("model path task failed: {e}")))??;

        let model_path_for_ctx = model_path.clone();
        let context = tokio::task::spawn_blocking(move || {
            WhisperContext::new_with_params(
                model_path_for_ctx.to_str().ok_or_else(|| {
                    SttError::ModelLoadError(format!(
                        "non-utf8 model path: {}",
                        model_path_for_ctx.display()
                    ))
                })?,
                WhisperContextParameters::default(),
            )
            .map_err(|e| {
                SttError::ModelLoadError(format!(
                    "failed to load whisper model from {}: {e}",
                    model_path_for_ctx.display()
                ))
            })
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("context task failed: {e}")))??;

        let mut state = self.state.write().await;
        state.config = Some(config);
        state.model_path = Some(model_path.clone());
        state.context = Some(Arc::new(context));

        info!(
            "{} initialized with model {}",
            runtime_name,
            model_path.display()
        );
        Ok(())
    }

    pub(crate) async fn transcribe(
        &self,
        audio_data: &[f32],
        format: AudioFormat,
    ) -> Result<Transcription> {
        let (config, context) = {
            let state = self.state.read().await;
            let config = state
                .config
                .clone()
                .ok_or_else(|| SttError::TranscriptionFailed("adapter not initialized".into()))?;
            let context = state.context.clone().ok_or_else(|| {
                SttError::TranscriptionFailed("model context not initialized".into())
            })?;
            (config, context)
        };

        let prepared_audio = prepare_audio(audio_data, &format);
        if prepared_audio.is_empty() {
            return Err(SttError::AudioError(
                "no audio samples available after preprocessing".into(),
            ));
        }

        debug!(
            "{} transcription started (raw_samples={}, prepared_samples={})",
            self.runtime_name,
            audio_data.len(),
            prepared_audio.len()
        );

        let language_override = config.language.clone();
        let task = config.task.clone();
        tokio::task::spawn_blocking(move || {
            run_whisper_transcription(context, prepared_audio, language_override, task)
        })
        .await
        .map_err(|e| SttError::TranscriptionFailed(format!("transcription task failed: {e}")))?
    }

    pub(crate) async fn is_model_available(&self, model_name: &str) -> bool {
        if looks_like_model_path(model_name) {
            return Path::new(model_name).exists();
        }

        model_cache_dir()
            .map(|dir| dir.join(model_filename(model_name)).exists())
            .unwrap_or(false)
    }

    pub(crate) fn available_models(&self) -> Vec<String> {
        vec![
            "tiny".to_string(),
            "tiny.en".to_string(),
            "base".to_string(),
            "base.en".to_string(),
            "small".to_string(),
            "small.en".to_string(),
            "medium".to_string(),
            "medium.en".to_string(),
            "large-v3-turbo".to_string(),
            "large-v3".to_string(),
        ]
    }

    pub(crate) fn current_model(&self) -> Option<String> {
        self.state
            .blocking_read()
            .config
            .as_ref()
            .map(|cfg| cfg.model_name.clone())
    }
}

fn run_whisper_transcription(
    context: Arc<WhisperContext>,
    audio_data: Vec<f32>,
    language_override: Option<String>,
    task: TranscriptionTask,
) -> Result<Transcription> {
    let mut state = context.create_state().map_err(|e| {
        SttError::TranscriptionFailed(format!("failed to create whisper state: {e}"))
    })?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(optimal_threads());
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_translate(matches!(task, TranscriptionTask::Translate));

    let language_option = language_override
        .as_deref()
        .map(str::trim)
        .filter(|lang| !lang.is_empty());
    if let Some(lang) = language_option {
        params.set_language(Some(lang));
        params.set_detect_language(false);
    } else {
        params.set_language(None);
        params.set_detect_language(true);
    }

    state
        .full(params, &audio_data)
        .map_err(|e| SttError::TranscriptionFailed(format!("whisper transcription failed: {e}")))?;

    let mut full_text = String::new();
    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let segment_text = segment
            .to_str_lossy()
            .map_err(|e| {
                SttError::TranscriptionFailed(format!("failed to read segment text: {e}"))
            })?
            .into_owned();

        full_text.push_str(&segment_text);
        let cleaned = segment_text.trim().to_string();
        if !cleaned.is_empty() {
            segments.push(TranscriptSegment {
                text: cleaned,
                start: segment.start_timestamp() as f64 / 100.0,
                end: segment.end_timestamp() as f64 / 100.0,
            });
        }
    }

    if full_text.trim().is_empty() && segments.is_empty() {
        warn!("whisper returned empty transcription result");
    }

    let language = if let Some(lang) = language_option {
        Some(lang.to_string())
    } else {
        get_lang_str(state.full_lang_id_from_state()).map(str::to_string)
    };

    Ok(Transcription {
        text: full_text.trim().to_string(),
        language,
        confidence: None,
        segments,
    })
}

fn optimal_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4) as i32
}

fn resolve_model_path(config: &SttConfig) -> Result<PathBuf> {
    if let Some(path) = config.model_path.clone() {
        if path.exists() {
            return Ok(path);
        }
        return Err(SttError::ModelNotFound(format!(
            "model path does not exist: {}",
            path.display()
        )));
    }

    if looks_like_model_path(&config.model_name) {
        let path = PathBuf::from(&config.model_name);
        if path.exists() {
            return Ok(path);
        }
        return Err(SttError::ModelNotFound(format!(
            "model path does not exist: {}",
            path.display()
        )));
    }

    let cache_dir = model_cache_dir()?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create model cache directory {}: {e}",
            cache_dir.display()
        ))
    })?;

    let model_path = cache_dir.join(model_filename(&config.model_name));
    if model_path.exists() {
        return Ok(model_path);
    }

    info!(
        "downloading model {} to {}",
        config.model_name,
        model_path.display()
    );
    download_model(&config.model_name, &model_path)?;
    Ok(model_path)
}

fn model_cache_dir() -> Result<PathBuf> {
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

fn download_model(model_name: &str, output_path: &Path) -> Result<()> {
    let filename = model_filename(model_name);
    let url = format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{filename}");

    let response = ureq::get(&url)
        .call()
        .map_err(|e| SttError::ModelLoadError(format!("failed to download {url}: {e}")))?;
    let mut reader = response.into_reader();

    let tmp_path = output_path.with_extension("download");
    let file = File::create(&tmp_path).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to create temporary model file {}: {e}",
            tmp_path.display()
        ))
    })?;
    let mut writer = BufWriter::new(file);

    io::copy(&mut reader, &mut writer).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed while writing model {}: {e}",
            output_path.display()
        ))
    })?;
    writer.flush().map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to flush downloaded model {}: {e}",
            output_path.display()
        ))
    })?;

    std::fs::rename(&tmp_path, output_path).map_err(|e| {
        SttError::ModelLoadError(format!(
            "failed to finalize downloaded model {}: {e}",
            output_path.display()
        ))
    })?;
    Ok(())
}

fn looks_like_model_path(model_name: &str) -> bool {
    model_name.contains('/')
        || model_name.contains('\\')
        || model_name.ends_with(".bin")
        || model_name.ends_with(".gguf")
}

pub(crate) fn model_filename(model_name: &str) -> String {
    if model_name.ends_with(".bin") {
        return model_name.to_string();
    }
    if model_name.starts_with("ggml-") {
        return format!("{model_name}.bin");
    }
    format!("ggml-{model_name}.bin")
}

pub(crate) fn prepare_audio(audio_data: &[f32], format: &AudioFormat) -> Vec<f32> {
    if audio_data.is_empty() || format.sample_rate == 0 || format.channels == 0 {
        return Vec::new();
    }

    let mono = if format.channels == 1 {
        audio_data.to_vec()
    } else {
        downmix_to_mono(audio_data, format.channels as usize)
    };

    if format.sample_rate == TARGET_SAMPLE_RATE {
        return mono;
    }

    resample_linear(&mono, format.sample_rate, TARGET_SAMPLE_RATE)
}

fn downmix_to_mono(audio_data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return audio_data.to_vec();
    }

    let mut mono = Vec::with_capacity(audio_data.len() / channels);
    for frame in audio_data.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / frame.len() as f32);
    }
    mono
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).max(1.0).round() as usize;

    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;

        let a = samples[idx.min(samples.len() - 1)];
        let b = samples[(idx + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_filename_maps_named_models() {
        assert_eq!(model_filename("base"), "ggml-base.bin");
        assert_eq!(model_filename("base.en"), "ggml-base.en.bin");
        assert_eq!(model_filename("small"), "ggml-small.bin");
    }

    #[test]
    fn model_filename_preserves_existing_bin_name() {
        assert_eq!(model_filename("ggml-custom.bin"), "ggml-custom.bin");
    }

    #[test]
    fn prepare_audio_downmixes_stereo_and_resamples_to_16k() {
        let input = vec![0.2, 0.6, 0.4, 0.8, 0.6, 1.0];
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };

        let out = prepare_audio(&input, &format);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.4).abs() < 0.001);
    }

    #[test]
    fn prepare_audio_passthroughs_16k_mono() {
        let input = vec![0.1, -0.2, 0.4, -0.6];
        let format = AudioFormat {
            sample_rate: TARGET_SAMPLE_RATE,
            channels: 1,
            bits_per_sample: 16,
        };
        let out = prepare_audio(&input, &format);
        assert_eq!(out, input);
    }
}
