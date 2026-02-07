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
    let language_option = language_override
        .as_deref()
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(str::to_string);

    let fast_attempt = decode_once(
        &context,
        &audio_data,
        language_option.as_deref(),
        &task,
        DecodeProfile::FastDictation,
    )?;
    println!(
        "[stt] fast decode chars={} segments={}",
        fast_attempt.text.chars().count(),
        fast_attempt.segments.len()
    );

    if !fast_attempt.text.trim().is_empty() || !fast_attempt.segments.is_empty() {
        return Ok(fast_attempt);
    }

    println!("[stt] fast decode empty, retrying with relaxed fallback");
    warn!("whisper returned empty text in fast mode, retrying with relaxed profile");
    let relaxed_attempt = decode_once(
        &context,
        &audio_data,
        language_option.as_deref(),
        &task,
        DecodeProfile::RelaxedFallback,
    )?;
    println!(
        "[stt] relaxed decode chars={} segments={}",
        relaxed_attempt.text.chars().count(),
        relaxed_attempt.segments.len()
    );

    if relaxed_attempt.text.trim().is_empty() && relaxed_attempt.segments.is_empty() {
        warn!("whisper returned empty transcription result after fallback profile");
    }

    Ok(relaxed_attempt)
}

enum DecodeProfile {
    FastDictation,
    RelaxedFallback,
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

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(optimal_threads());
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_translate(matches!(task, TranscriptionTask::Translate));

    match profile {
        DecodeProfile::FastDictation => {
            // Optimize for quick push-to-talk commands.
            params.set_no_timestamps(true);
            params.set_single_segment(true);
            params.set_no_context(true);
            params.set_max_initial_ts(3.0);
            params.set_suppress_blank(false);
            params.set_no_speech_thold(1.0);
            params.set_logprob_thold(-99.0);
        }
        DecodeProfile::RelaxedFallback => {
            // Recover text from short clips with leading silence.
            params.set_no_timestamps(false);
            params.set_single_segment(false);
            params.set_no_context(false);
            params.set_max_initial_ts(8.0);
            params.set_suppress_blank(false);
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

    let mut text = String::new();
    let mut segments = Vec::new();
    for segment in state.as_iter() {
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

    let mut prepared = if format.sample_rate == TARGET_SAMPLE_RATE {
        mono
    } else {
        resample_linear(&mono, format.sample_rate, TARGET_SAMPLE_RATE)
    };

    normalize_for_asr(&mut prepared);
    trim_silence(prepared)
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

fn trim_silence(samples: Vec<f32>) -> Vec<f32> {
    if samples.is_empty() {
        return samples;
    }

    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, |acc, v| acc.max(v));
    if peak <= f32::EPSILON {
        return samples;
    }

    let threshold = (peak * 0.08).max(0.002);
    let first = match samples.iter().position(|s| s.abs() >= threshold) {
        Some(idx) => idx,
        None => return samples,
    };
    let last = match samples.iter().rposition(|s| s.abs() >= threshold) {
        Some(idx) => idx,
        None => return samples,
    };

    let pad = (TARGET_SAMPLE_RATE as usize) / 8;
    let start = first.saturating_sub(pad);
    let end = (last + pad + 1).min(samples.len());

    if start == 0 && end == samples.len() {
        return samples;
    }

    samples[start..end].to_vec()
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
    fn trim_silence_removes_leading_and_trailing_quiet_sections() {
        let mut input = vec![0.0; 4000];
        input.extend_from_slice(&[0.3, -0.25, 0.2, -0.15]);
        input.extend(vec![0.0; 4000]);

        let out = trim_silence(input.clone());
        assert!(out.len() < input.len());
        assert!(out.iter().any(|s| s.abs() > 0.1));
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
