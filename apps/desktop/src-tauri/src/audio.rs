use arboard::{Clipboard, ImageData};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use enigo::{Enigo, Key, KeyboardControllable};
use serde::Serialize;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use stt::{create_adapter, AudioFormat as SttAudioFormat, SttAdapter, SttConfig};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsWindow, SetForegroundWindow,
};

// Simple wrapper to make Stream thread-safe
struct AudioStream {
    stream: Option<Stream>,
}

unsafe impl Send for AudioStream {}
unsafe impl Sync for AudioStream {}

pub struct AudioCapture {
    stream: Arc<Mutex<AudioStream>>,
    samples: Arc<Mutex<Vec<f32>>>,
    format: Arc<Mutex<SttAudioFormat>>,
    stt_adapter: Arc<AsyncMutex<Option<Box<dyn SttAdapter>>>>,
    loaded_model: Arc<AsyncMutex<Option<String>>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: Arc::new(Mutex::new(AudioStream { stream: None })),
            samples: Arc::new(Mutex::new(Vec::new())),
            format: Arc::new(Mutex::new(SttAudioFormat::default())),
            stt_adapter: Arc::new(AsyncMutex::new(None)),
            loaded_model: Arc::new(AsyncMutex::new(None)),
        }
    }
}

impl Clone for AudioCapture {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.clone(),
            samples: self.samples.clone(),
            format: self.format.clone(),
            stt_adapter: self.stt_adapter.clone(),
            loaded_model: self.loaded_model.clone(),
        }
    }
}

#[derive(Clone, Serialize)]
struct TranscriptionStatusEvent {
    status: String,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct TranscriptionResultEvent {
    text: String,
    language: Option<String>,
    confidence: Option<f32>,
    is_final: bool,
}

fn emit_transcription_status(app: &AppHandle, status: &str, error: Option<String>) {
    let _ = app.emit_all(
        "transcription-status",
        TranscriptionStatusEvent {
            status: status.to_string(),
            error,
        },
    );
}

enum ClipboardSnapshot {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
    OpaqueOrEmpty,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct MacPasteTarget {
    pid: i32,
    name: String,
}

#[cfg(target_os = "macos")]
fn parse_frontmost_pid(raw: &str) -> Option<i32> {
    let parsed = raw.trim().parse::<i32>().ok()?;
    if parsed <= 0 {
        return None;
    }
    Some(parsed)
}

#[cfg(target_os = "macos")]
fn paste_target_slot() -> &'static Mutex<Option<MacPasteTarget>> {
    static SLOT: OnceLock<Mutex<Option<MacPasteTarget>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "windows")]
fn paste_target_slot() -> &'static Mutex<Option<isize>> {
    static SLOT: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn paste_target_slot() -> &'static Mutex<Option<()>> {
    static SLOT: OnceLock<Mutex<Option<()>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "macos")]
fn capture_active_paste_target() {
    let pid_output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get unix id of first application process whose frontmost is true")
        .output();
    let Ok(pid_output) = pid_output else {
        eprintln!("[paste] could not query frontmost app pid with osascript");
        return;
    };
    if !pid_output.status.success() {
        eprintln!("[paste] osascript query for frontmost app pid failed");
        return;
    }

    let Ok(raw_pid) = String::from_utf8(pid_output.stdout) else {
        return;
    };
    let Some(pid) = parse_frontmost_pid(&raw_pid) else {
        eprintln!("[paste] invalid frontmost app pid '{}'", raw_pid.trim());
        return;
    };
    let self_pid = std::process::id() as i32;
    if pid == self_pid {
        eprintln!(
            "[paste] ignoring self pid {} as paste target (openwispr frontmost)",
            pid
        );
        return;
    }

    let name = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "<unknown>".to_string());

    if let Ok(mut slot) = paste_target_slot().lock() {
        println!("[paste] captured frontmost app pid={} name='{}'", pid, name);
        *slot = Some(MacPasteTarget { pid, name });
    }
}

#[cfg(target_os = "windows")]
fn capture_active_paste_target() {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        eprintln!("[paste] GetForegroundWindow returned null");
        return;
    }
    if let Ok(mut slot) = paste_target_slot().lock() {
        println!("[paste] captured foreground HWND 0x{:X}", hwnd as usize);
        *slot = Some(hwnd);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_active_paste_target() {}

pub fn remember_active_paste_target() {
    capture_active_paste_target();
}

#[cfg(target_os = "macos")]
fn restore_active_paste_target() {
    let target = paste_target_slot().lock().ok().and_then(|slot| slot.clone());
    let Some(target) = target else {
        eprintln!("[paste] no captured app to restore on macOS");
        return;
    };

    let result = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to set frontmost of (first application process whose unix id is {}) to true",
            target.pid
        ))
        .status();
    if result.as_ref().is_ok_and(|status| status.success()) {
        println!(
            "[paste] restored focus to app pid={} name='{}'",
            target.pid, target.name
        );
    } else {
        eprintln!(
            "[paste] failed to restore focus to app pid={} name='{}'",
            target.pid, target.name
        );
    }
    thread::sleep(Duration::from_millis(45));
}

#[cfg(target_os = "windows")]
fn restore_active_paste_target() {
    let target = paste_target_slot().lock().ok().and_then(|slot| *slot);
    let Some(hwnd) = target else {
        eprintln!("[paste] no captured HWND to restore on Windows");
        return;
    };

    let valid = unsafe { IsWindow(hwnd) != 0 };
    if !valid {
        eprintln!("[paste] captured HWND is no longer valid");
        return;
    }

    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    println!("[paste] restored focus to HWND 0x{:X}", hwnd as usize);
    thread::sleep(Duration::from_millis(45));
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn restore_active_paste_target() {}

fn capture_clipboard(clipboard: &mut Clipboard) -> ClipboardSnapshot {
    if let Ok(text) = clipboard.get_text() {
        return ClipboardSnapshot::Text(text);
    }

    if let Ok(image) = clipboard.get_image() {
        return ClipboardSnapshot::Image {
            width: image.width,
            height: image.height,
            bytes: image.bytes.as_ref().to_vec(),
        };
    }

    ClipboardSnapshot::OpaqueOrEmpty
}

fn restore_clipboard(clipboard: &mut Clipboard, snapshot: ClipboardSnapshot) {
    match snapshot {
        ClipboardSnapshot::Text(text) => {
            let _ = clipboard.set_text(text);
        }
        ClipboardSnapshot::Image {
            width,
            height,
            bytes,
        } => {
            let _ = clipboard.set_image(ImageData {
                width,
                height,
                bytes: Cow::Owned(bytes),
            });
        }
        ClipboardSnapshot::OpaqueOrEmpty => {}
    }
}

fn trigger_paste_shortcut() {
    let mut enigo = Enigo::new();
    #[cfg(target_os = "macos")]
    {
        enigo.key_down(Key::Meta);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Meta);
    }
    #[cfg(not(target_os = "macos"))]
    {
        enigo.key_down(Key::Control);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Control);
    }
}

fn insert_text_directly(text: &str) {
    let mut enigo = Enigo::new();
    enigo.key_sequence(text);
}

fn paste_text_preserving_clipboard(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }

    // The overlay can become the active app while transcribing. Move focus back
    // to the app that was active at Fn press before injecting text.
    restore_active_paste_target();

    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard unavailable: {}", e))?;
    let snapshot = capture_clipboard(&mut clipboard);

    if let ClipboardSnapshot::OpaqueOrEmpty = snapshot {
        insert_text_directly(text);
        return Ok(());
    }

    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;

    // Let clipboard managers receive the new value before pasting.
    thread::sleep(Duration::from_millis(35));
    trigger_paste_shortcut();
    // Let target app consume Cmd/Ctrl+V before restoring clipboard.
    thread::sleep(Duration::from_millis(110));
    restore_clipboard(&mut clipboard, snapshot);

    Ok(())
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn select_input_device(host: &Host) -> Result<Device, String> {
    if let Ok(requested) = std::env::var("OPENWISPR_INPUT_DEVICE") {
        let needle = requested.trim().to_lowercase();
        if !needle.is_empty() {
            let devices = host
                .input_devices()
                .map_err(|e| format!("Failed to enumerate input devices: {}", e))?;
            for device in devices {
                let name = device
                    .name()
                    .unwrap_or_else(|_| "<unknown input device>".to_string());
                if name.to_lowercase().contains(&needle) {
                    println!("[audio] selected input device by env override: {}", name);
                    return Ok(device);
                }
            }
            return Err(format!(
                "No input device matching OPENWISPR_INPUT_DEVICE='{}'",
                requested
            ));
        }
    }

    let default = host.default_input_device();
    if let Some(device) = default {
        let name = device
            .name()
            .unwrap_or_else(|_| "<unknown input device>".to_string());
        println!("[audio] selected default input device: {}", name);
        return Ok(device);
    }

    Err("No input device available".to_string())
}

fn ffmpeg_binary_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        return &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe", "ffmpeg"];
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["ffmpeg", "ffmpeg-aarch64-apple-darwin"]
    }
}

fn resolve_ffmpeg_binary() -> Option<String> {
    static FFMPEG_BIN: OnceLock<Option<String>> = OnceLock::new();
    if let Some(cached) = FFMPEG_BIN.get() {
        return cached.clone();
    }

    if let Ok(custom) = std::env::var("OPENWISPR_FFMPEG_BIN") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            let resolved = Some(trimmed.to_string());
            let _ = FFMPEG_BIN.set(resolved.clone());
            return resolved;
        }
    }

    let resolved = ffmpeg_binary_candidates()
        .iter()
        .find_map(|candidate| {
            Command::new(candidate)
                .arg("-version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|_| (*candidate).to_string())
        });
    let _ = FFMPEG_BIN.set(resolved.clone());
    resolved
}

fn ffmpeg_normalize_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-hide_banner".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-ar".to_string(),
        "16000".to_string(),
        "-sample_fmt".to_string(),
        "s16".to_string(),
        output.to_string_lossy().to_string(),
    ]
}

fn write_wav_from_f32(path: &Path, samples: &[f32], format: &SttAudioFormat) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: format.channels.max(1),
        sample_rate: format.sample_rate.max(1),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("Failed to create wav: {}", e))?;

    for sample in samples {
        let scaled = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32);
        writer
            .write_sample(scaled as i16)
            .map_err(|e| format!("Failed to write wav sample: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize wav: {}", e))?;
    Ok(())
}

fn read_wav_to_f32(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open wav: {}", e))?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(format!(
            "Unexpected normalized wav format: {:?} {}-bit",
            spec.sample_format, spec.bits_per_sample
        ));
    }
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read normalized wav samples: {}", e))?;
    Ok(samples
        .into_iter()
        .map(|s| s as f32 / i16::MAX as f32)
        .collect())
}

fn normalize_audio_for_stt_with_ffmpeg(
    audio_data: &[f32],
    format: &SttAudioFormat,
) -> Result<(Vec<f32>, SttAudioFormat), String> {
    let ffmpeg = resolve_ffmpeg_binary().ok_or_else(|| "ffmpeg binary not found".to_string())?;

    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Clock error: {}", e))?
        .as_millis();
    let pid = std::process::id();
    let temp_dir = std::env::temp_dir().join("openwispr");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let input_path = temp_dir.join(format!("raw_{pid}_{millis}.wav"));
    let output_path = temp_dir.join(format!("normalized_{pid}_{millis}.wav"));

    write_wav_from_f32(&input_path, audio_data, format)?;

    let args = ffmpeg_normalize_args(&input_path, &output_path);
    let status = Command::new(&ffmpeg)
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to spawn ffmpeg '{}': {}", ffmpeg, e))?;

    if !status.success() {
        let _ = fs::remove_file(&input_path);
        let _ = fs::remove_file(&output_path);
        return Err(format!(
            "ffmpeg normalization failed with status {:?}",
            status.code()
        ));
    }

    let normalized_samples = read_wav_to_f32(&output_path)?;
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);

    Ok((
        normalized_samples,
        SttAudioFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
    ))
}

pub fn start_recording_for_capture(capture: &AudioCapture, app: AppHandle) -> Result<(), String> {
    let mut stream_lock = capture.stream.lock().unwrap();
    if stream_lock.stream.is_some() {
        return Ok(());
    }

    // Get the default audio host
    let host = cpal::default_host();

    // Get the selected input device
    let device = select_input_device(&host)?;

    // Get the default input config
    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;
    println!(
        "[audio] input format sample_rate={} channels={} sample_format={:?}",
        config.sample_rate().0,
        config.channels(),
        config.sample_format()
    );

    // Reset buffered samples and capture format for the next transcription run.
    {
        let mut samples = capture.samples.lock().unwrap();
        samples.clear();
    }
    {
        let mut format = capture.format.lock().unwrap();
        *format = SttAudioFormat {
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
            bits_per_sample: 16,
        };
    }

    emit_transcription_status(&app, "listening", None);

    // Build the input stream
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_input_stream::<f32>(&device, &config.into(), app, capture.samples.clone())?
        }
        cpal::SampleFormat::I16 => {
            build_input_stream::<i16>(&device, &config.into(), app, capture.samples.clone())?
        }
        cpal::SampleFormat::U16 => {
            build_input_stream::<u16>(&device, &config.into(), app, capture.samples.clone())?
        }
        _ => return Err("Unsupported sample format".to_string()),
    };

    stream
        .play()
        .map_err(|e| format!("Failed to play stream: {}", e))?;

    stream_lock.stream = Some(stream);

    Ok(())
}

#[tauri::command]
pub fn start_recording(state: tauri::State<AudioCapture>, app: AppHandle) -> Result<(), String> {
    start_recording_for_capture(state.inner(), app)
}

fn build_input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    app: AppHandle,
    stt_samples: Arc<Mutex<Vec<f32>>>,
) -> Result<Stream, String>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let err_fn = |err| eprintln!("Error in audio stream: {}", err);

    let mut buffer = Vec::new();
    let chunk_size = (config.sample_rate.0 as f32 * 0.1) as usize; // 100ms chunks

    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Convert samples to f32
                let samples: Vec<f32> =
                    data.iter().map(|&s| cpal::Sample::from_sample(s)).collect();
                if let Ok(mut stt_data) = stt_samples.lock() {
                    stt_data.extend_from_slice(&samples);
                }

                buffer.extend_from_slice(&samples);

                // Process in chunks
                if buffer.len() >= chunk_size {
                    let rms = calculate_rms(&buffer[..chunk_size]);

                    // Convert RMS to dB (approximate)
                    let db = if rms > 0.0 {
                        20.0 * rms.log10()
                    } else {
                        -100.0
                    };

                    // Normalize to 0-100 range for UI
                    // Typical range: -60 dB (quiet) to 0 dB (loud)
                    let level = ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);

                    // Emit audio level event
                    let _ = app.emit_all("audio-level", level);

                    // Clear processed samples
                    buffer.drain(..chunk_size);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

    Ok(stream)
}

pub async fn stop_recording_for_capture(
    capture: AudioCapture,
    app: AppHandle,
) -> Result<(), String> {
    let had_stream = {
        let mut stream_lock = capture.stream.lock().unwrap();
        stream_lock.stream.take().is_some()
    };
    if !had_stream {
        return Ok(());
    }

    emit_transcription_status(&app, "processing", None);

    let audio_data = {
        let mut samples = capture.samples.lock().unwrap();
        std::mem::take(&mut *samples)
    };

    if audio_data.is_empty() {
        println!("[stt] no audio captured, skipping transcription");
        emit_transcription_status(&app, "idle", None);
        return Ok(());
    }

    let format = {
        let format = capture.format.lock().unwrap();
        format.clone()
    };
    let target_model = crate::models::active_model_value();
    let mut adapter_guard = capture.stt_adapter.lock().await;
    let mut loaded_model_guard = capture.loaded_model.lock().await;
    if adapter_guard.is_none() || loaded_model_guard.as_deref() != Some(target_model.as_str()) {
        let mut adapter = create_adapter().map_err(|e| e.to_string())?;
        adapter
            .initialize(SttConfig {
                model_name: target_model.clone(),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        *adapter_guard = Some(adapter);
        *loaded_model_guard = Some(target_model);
    }
    let adapter = adapter_guard
        .as_ref()
        .ok_or_else(|| "STT adapter unavailable".to_string())?;

    let model_name = loaded_model_guard
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let (audio_data, format) = match normalize_audio_for_stt_with_ffmpeg(&audio_data, &format) {
        Ok((samples, normalized_format)) => {
            println!(
                "[stt] ffmpeg normalization applied samples={} sample_rate={} channels={}",
                samples.len(),
                normalized_format.sample_rate,
                normalized_format.channels
            );
            (samples, normalized_format)
        }
        Err(err) => {
            eprintln!("[stt] ffmpeg normalization unavailable, using raw capture: {}", err);
            (audio_data, format)
        }
    };
    let audio_seconds = if format.sample_rate > 0 && format.channels > 0 {
        audio_data.len() as f32 / format.sample_rate as f32 / format.channels as f32
    } else {
        0.0
    };
    println!(
        "[stt] transcription started model={} samples={} duration_s={:.2} sample_rate={} channels={}",
        model_name,
        audio_data.len(),
        audio_seconds,
        format.sample_rate,
        format.channels
    );

    match adapter.transcribe(&audio_data, format).await {
        Ok(result) => {
            let language = result
                .language
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let confidence = result
                .confidence
                .map(|value| format!("{:.3}", value))
                .unwrap_or_else(|| "n/a".to_string());
            println!(
                "[stt] transcription complete model={} language={} confidence={} chars={}",
                model_name,
                language,
                confidence,
                result.text.chars().count()
            );
            println!("[stt] transcript: {}", result.text);

            if let Err(err) = paste_text_preserving_clipboard(&result.text) {
                eprintln!("[stt] paste failed: {}", err);
                emit_transcription_status(&app, "error", Some(err.clone()));
                return Err(err);
            }

            let _ = app.emit_all(
                "transcription-result",
                TranscriptionResultEvent {
                    text: result.text,
                    language: result.language,
                    confidence: result.confidence,
                    is_final: true,
                },
            );
            emit_transcription_status(&app, "idle", None);
            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            eprintln!("[stt] transcription failed: {}", message);
            emit_transcription_status(&app, "error", Some(message.clone()));
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, AudioCapture>,
    app: AppHandle,
) -> Result<(), String> {
    stop_recording_for_capture(state.inner().clone(), app).await
}

#[cfg(test)]
mod tests {
    use super::ffmpeg_normalize_args;
    #[cfg(target_os = "macos")]
    use super::parse_frontmost_pid;
    use std::path::Path;

    #[test]
    fn ffmpeg_normalize_args_target_whisper_contract() {
        let args = ffmpeg_normalize_args(Path::new("in.wav"), Path::new("out.wav"));
        assert!(args.iter().any(|arg| arg == "-ac"));
        assert!(args.iter().any(|arg| arg == "1"));
        assert!(args.iter().any(|arg| arg == "-ar"));
        assert!(args.iter().any(|arg| arg == "16000"));
        assert!(args.iter().any(|arg| arg == "-sample_fmt"));
        assert!(args.iter().any(|arg| arg == "s16"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_frontmost_pid_filters_invalid_values() {
        assert_eq!(parse_frontmost_pid(""), None);
        assert_eq!(parse_frontmost_pid("  \n"), None);
        assert_eq!(parse_frontmost_pid("-1"), None);
        assert_eq!(parse_frontmost_pid("0"), None);
        assert_eq!(parse_frontmost_pid("1234\n"), Some(1234));
    }
}
