use crate::{
    emit_model_download_progress, AudioFormat, ModelDownloadProgress, Result, SttConfig, SttError,
    TranscriptSegment, Transcription, TranscriptionTask,
};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use whisper_rs::{
    get_lang_str, install_logging_hooks, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
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

fn verbose_logs_enabled() -> bool {
    std::env::var("OPENWISPR_VERBOSE_LOGS")
        .ok()
        .as_deref()
        .map(|v| v == "1")
        .unwrap_or(false)
}

impl SharedWhisperAdapter {
    pub(crate) fn new(runtime_name: &'static str) -> Self {
        Self {
            runtime_name,
            state: Arc::new(RwLock::new(SharedState::default())),
        }
    }

    pub(crate) async fn initialize(&self, config: SttConfig) -> Result<()> {
        // Route whisper.cpp / ggml logs through whisper-rs hooks. Without a logging backend
        // enabled, this effectively silences native library spam in terminal output.
        install_logging_hooks();

        let runtime_name = self.runtime_name;
        let model_path = tokio::task::spawn_blocking({
            let config = config.clone();
            move || resolve_model_path(&config)
        })
        .await
        .map_err(|e| SttError::ModelLoadError(format!("model path task failed: {e}")))??;

        let model_path_for_ctx = model_path.clone();
        let (prefer_gpu, preferred_backend) = preferred_backend();
        let context = tokio::task::spawn_blocking(move || {
            let model_path_str = model_path_for_ctx.to_str().ok_or_else(|| {
                SttError::ModelLoadError(format!(
                    "non-utf8 model path: {}",
                    model_path_for_ctx.display()
                ))
            })?;

            let mut params = WhisperContextParameters::default();
            params.use_gpu(prefer_gpu);
            match WhisperContext::new_with_params(model_path_str, params) {
                Ok(ctx) => {
                    info!(
                        "whisper context initialized with backend={} (gpu_requested={})",
                        preferred_backend,
                        prefer_gpu
                    );
                    Ok(ctx)
                }
                Err(gpu_err) if prefer_gpu => {
                    warn!(
                        "failed to initialize whisper with backend={}; retrying on CPU: {}",
                        preferred_backend,
                        gpu_err
                    );

                    let mut cpu_params = WhisperContextParameters::default();
                    cpu_params.use_gpu(false);
                    WhisperContext::new_with_params(model_path_str, cpu_params).map_err(|cpu_err| {
                        SttError::ModelLoadError(format!(
                            "failed to load whisper model from {} with GPU ({}) and CPU fallback (cpu): {}",
                            model_path_for_ctx.display(),
                            preferred_backend,
                            cpu_err
                        ))
                    })
                }
                Err(err) => Err(SttError::ModelLoadError(format!(
                    "failed to load whisper model from {}: {err}",
                    model_path_for_ctx.display()
                ))),
            }
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

        if verbose_logs_enabled() {
            let raw_stats = signal_stats(audio_data);
            let prepared_stats = signal_stats(&prepared_audio);
            println!(
                "[stt] signal raw: samples={} peak={:.5} rms={:.5} zcr={:.5}",
                audio_data.len(),
                raw_stats.peak,
                raw_stats.rms,
                raw_stats.zero_crossing_rate
            );
            println!(
                "[stt] signal prepared: samples={} peak={:.5} rms={:.5} zcr={:.5}",
                prepared_audio.len(),
                prepared_stats.peak,
                prepared_stats.rms,
                prepared_stats.zero_crossing_rate
            );
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

}

fn preferred_backend() -> (bool, &'static str) {
    #[cfg(target_os = "macos")]
    {
        let is_apple_silicon = std::env::consts::ARCH == "aarch64";
        if is_apple_silicon {
            return (true, "metal");
        }
        return (false, "cpu");
    }

    #[cfg(target_os = "windows")]
    {
        return (true, "vulkan");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        (false, "cpu")
    }
}

fn run_whisper_transcription(
    context: Arc<WhisperContext>,
    audio_data: Vec<f32>,
    language_override: Option<String>,
    task: TranscriptionTask,
) -> Result<Transcription> {
    let requested_language = language_override
        .as_deref()
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(str::to_string);
    let preferred_language = requested_language
        .clone()
        .or_else(|| Some("en".to_string()));

    let primary_attempt = decode_once(
        &context,
        &audio_data,
        preferred_language.as_deref(),
        &task,
        DecodeProfile::Primary,
    )?;
    println!(
        "[stt] primary decode chars={} segments={} lang={}",
        primary_attempt.text.chars().count(),
        primary_attempt.segments.len(),
        preferred_language.as_deref().unwrap_or("auto")
    );
    if !primary_attempt.text.trim().is_empty() || !primary_attempt.segments.is_empty() {
        return Ok(primary_attempt);
    }

    if requested_language.is_none() {
        if verbose_logs_enabled() {
            println!("[stt] primary decode empty, retrying with auto language detection");
        }
        let auto_attempt = decode_once(
            &context,
            &audio_data,
            None,
            &task,
            DecodeProfile::Primary,
        )?;
        if verbose_logs_enabled() {
            println!(
                "[stt] auto-language decode chars={} segments={}",
                auto_attempt.text.chars().count(),
                auto_attempt.segments.len()
            );
        }
        if !auto_attempt.text.trim().is_empty() || !auto_attempt.segments.is_empty() {
            return Ok(auto_attempt);
        }
    }

    if verbose_logs_enabled() {
        println!("[stt] primary decode empty, retrying with permissive fallback");
    }
    let permissive_attempt = decode_once(
        &context,
        &audio_data,
        preferred_language.as_deref(),
        &task,
        DecodeProfile::PermissiveFallback,
    )?;
    if verbose_logs_enabled() {
        println!(
            "[stt] permissive decode chars={} segments={}",
            permissive_attempt.text.chars().count(),
            permissive_attempt.segments.len()
        );
    }

    if permissive_attempt.text.trim().is_empty() && permissive_attempt.segments.is_empty() {
        warn!("whisper returned empty transcription result after all decode attempts");
    }
    Ok(permissive_attempt)
}

enum DecodeProfile {
    Primary,
    PermissiveFallback,
}

fn decode_once(
    context: &Arc<WhisperContext>,
    audio_data: &[f32],
    language_option: Option<&str>,
    task: &TranscriptionTask,
    profile: DecodeProfile,
) -> Result<Transcription> {
    let mut state = context.create_state().map_err(|e| {
        SttError::TranscriptionFailed(format!("failed to create whisper state: {e}"))
    })?;

    let mut params = match profile {
        DecodeProfile::Primary => FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        }),
        DecodeProfile::PermissiveFallback => {
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        }
    };
    params.set_n_threads(optimal_threads());
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_timestamps(false);
    params.set_single_segment(false);
    params.set_no_context(false);
    params.set_translate(matches!(task, TranscriptionTask::Translate));
    params.set_temperature(0.2);
    params.set_temperature_inc(0.2);
    params.set_max_initial_ts(1.0);
    params.set_entropy_thold(2.4);
    params.set_initial_prompt("");

    match profile {
        DecodeProfile::Primary => {
            // Mirrors voicetypr defaults for stable dictation output.
            params.set_suppress_blank(true);
            params.set_suppress_nst(true);
            params.set_no_speech_thold(0.6);
            params.set_logprob_thold(-1.0);
        }
        DecodeProfile::PermissiveFallback => {
            // Reduce filtering when the primary profile yields empty text.
            params.set_suppress_blank(false);
            params.set_suppress_nst(false);
            params.set_no_speech_thold(1.0);
            params.set_logprob_thold(-99.0);
        }
    }

    if let Some(lang) = language_option {
        params.set_language(Some(lang));
        params.set_detect_language(false);
    } else {
        params.set_language(None);
        params.set_detect_language(true);
    }

    state
        .full(params, audio_data)
        .map_err(|e| SttError::TranscriptionFailed(format!("whisper transcription failed: {e}")))?;

    let n_segments = state.full_n_segments();
    let mut text = String::new();
    let mut segments = Vec::new();
    for i in 0..n_segments {
        let Some(segment) = state.get_segment(i) else {
            continue;
        };
        let segment_text = segment
            .to_str_lossy()
            .map_err(|e| {
                SttError::TranscriptionFailed(format!("failed to read segment text: {e}"))
            })?
            .into_owned();
        text.push_str(&segment_text);

        let cleaned = segment_text.trim().to_string();
        if !cleaned.is_empty() {
            segments.push(TranscriptSegment {
                text: cleaned,
                start: segment.start_timestamp() as f64 / 100.0,
                end: segment.end_timestamp() as f64 / 100.0,
            });
        }
    }

    let language = if let Some(lang) = language_option {
        Some(lang.to_string())
    } else {
        get_lang_str(state.full_lang_id_from_state()).map(str::to_string)
    };

    Ok(Transcription {
        text: text.trim().to_string(),
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
        .map_err(|e| {
            let message = format!("failed to download {url}: {e}");
            emit_model_download_progress(ModelDownloadProgress {
                model_name: model_name.to_string(),
                stage: "download".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                percent: None,
                done: true,
                error: Some(message.clone()),
                message: Some("Download request failed".to_string()),
            });
            SttError::ModelLoadError(message)
        })?;
    let total_bytes = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);

    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_name.to_string(),
        stage: "download".to_string(),
        downloaded_bytes: 0,
        total_bytes,
        percent: Some(0.0),
        done: false,
        error: None,
        message: Some("Starting model download".to_string()),
    });

    let mut reader = response.into_reader();

    let tmp_path = output_path.with_extension("download");
    let file = File::create(&tmp_path).map_err(|e| {
        let message = format!(
            "failed to create temporary model file {}: {e}",
            tmp_path.display()
        );
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_name.to_string(),
            stage: "download".to_string(),
            downloaded_bytes: 0,
            total_bytes,
            percent: None,
            done: true,
            error: Some(message.clone()),
            message: Some("Failed to create temporary file".to_string()),
        });
        SttError::ModelLoadError(message)
    })?;
    let mut writer = BufWriter::new(file);

    let mut downloaded_bytes = 0_u64;
    let mut last_emitted = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| {
            let message = format!("failed while reading model stream {}: {e}", output_path.display());
            emit_model_download_progress(ModelDownloadProgress {
                model_name: model_name.to_string(),
                stage: "download".to_string(),
                downloaded_bytes,
                total_bytes,
                percent: total_bytes.map(|t| ((downloaded_bytes as f32 / t as f32) * 100.0).min(100.0)),
                done: true,
                error: Some(message.clone()),
                message: Some("Download stream read failed".to_string()),
            });
            SttError::ModelLoadError(message)
        })?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n]).map_err(|e| {
            let message = format!("failed while writing model {}: {e}", output_path.display());
            emit_model_download_progress(ModelDownloadProgress {
                model_name: model_name.to_string(),
                stage: "download".to_string(),
                downloaded_bytes,
                total_bytes,
                percent: total_bytes.map(|t| ((downloaded_bytes as f32 / t as f32) * 100.0).min(100.0)),
                done: true,
                error: Some(message.clone()),
                message: Some("Write to temporary file failed".to_string()),
            });
            SttError::ModelLoadError(message)
        })?;
        downloaded_bytes += n as u64;

        if downloaded_bytes.saturating_sub(last_emitted) >= 256 * 1024
            || total_bytes.is_some_and(|total| downloaded_bytes >= total)
        {
            emit_model_download_progress(ModelDownloadProgress {
                model_name: model_name.to_string(),
                stage: "download".to_string(),
                downloaded_bytes,
                total_bytes,
                percent: total_bytes.map(|t| ((downloaded_bytes as f32 / t as f32) * 100.0).min(100.0)),
                done: false,
                error: None,
                message: Some("Downloading model".to_string()),
            });
            last_emitted = downloaded_bytes;
        }
    }

    writer.flush().map_err(|e| {
        let message = format!(
            "failed to flush downloaded model {}: {e}",
            output_path.display()
        );
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_name.to_string(),
            stage: "download".to_string(),
            downloaded_bytes,
            total_bytes,
            percent: total_bytes.map(|t| ((downloaded_bytes as f32 / t as f32) * 100.0).min(100.0)),
            done: true,
            error: Some(message.clone()),
            message: Some("Failed to flush temporary file".to_string()),
        });
        SttError::ModelLoadError(message)
    })?;

    std::fs::rename(&tmp_path, output_path).map_err(|e| {
        let message = format!(
            "failed to finalize downloaded model {}: {e}",
            output_path.display()
        );
        emit_model_download_progress(ModelDownloadProgress {
            model_name: model_name.to_string(),
            stage: "download".to_string(),
            downloaded_bytes,
            total_bytes,
            percent: total_bytes.map(|t| ((downloaded_bytes as f32 / t as f32) * 100.0).min(100.0)),
            done: true,
            error: Some(message.clone()),
            message: Some("Failed to finalize model file".to_string()),
        });
        SttError::ModelLoadError(message)
    })?;

    emit_model_download_progress(ModelDownloadProgress {
        model_name: model_name.to_string(),
        stage: "ready".to_string(),
        downloaded_bytes,
        total_bytes,
        percent: Some(100.0),
        done: true,
        error: None,
        message: Some("Model download complete".to_string()),
    });

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

    let mut prepared = if format.sample_rate == TARGET_SAMPLE_RATE {
        mono
    } else {
        resample_linear(&mono, format.sample_rate, TARGET_SAMPLE_RATE)
    };

    normalize_for_asr(&mut prepared);
    prepared
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

fn normalize_for_asr(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }

    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, |acc, v| acc.max(v));
    if peak <= f32::EPSILON {
        return;
    }

    // Leave normal/loud captures untouched; only lift very quiet push-to-talk clips.
    if peak >= 0.20 {
        return;
    }

    let target_peak = 0.35_f32;
    let gain = (target_peak / peak).clamp(1.0, 80.0);
    if (gain - 1.0).abs() < 0.01 {
        return;
    }

    for sample in samples.iter_mut() {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SignalStats {
    peak: f32,
    rms: f32,
    zero_crossing_rate: f32,
}

fn signal_stats(samples: &[f32]) -> SignalStats {
    if samples.is_empty() {
        return SignalStats::default();
    }

    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, |acc, v| acc.max(v));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

    let mut crossings = 0usize;
    for pair in samples.windows(2) {
        let a = pair[0];
        let b = pair[1];
        if (a >= 0.0 && b < 0.0) || (a < 0.0 && b >= 0.0) {
            crossings += 1;
        }
    }
    let zcr = crossings as f32 / samples.len() as f32;

    SignalStats {
        peak,
        rms,
        zero_crossing_rate: zcr,
    }
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

    #[test]
    fn prepare_audio_normalizes_very_quiet_input() {
        let input = vec![0.001, -0.0015, 0.002];
        let format = AudioFormat {
            sample_rate: TARGET_SAMPLE_RATE,
            channels: 1,
            bits_per_sample: 16,
        };

        let out = prepare_audio(&input, &format);
        let max_amp = out
            .iter()
            .map(|s| s.abs())
            .fold(0.0_f32, |acc, v| acc.max(v));

        // Push-to-talk clips can be quiet; preprocessing should boost them.
        assert!(max_amp > 0.1, "expected normalized output, got max_amp={max_amp}");
        assert!(max_amp <= 1.0, "normalized output should remain in range");
    }

    #[test]
    fn signal_stats_reports_peak_and_rms() {
        let input = vec![0.5, -0.5, 0.5, -0.5];
        let stats = signal_stats(&input);
        assert!((stats.peak - 0.5).abs() < 0.0001);
        assert!((stats.rms - 0.5).abs() < 0.0001);
        assert!(stats.zero_crossing_rate > 0.0);
    }
}
