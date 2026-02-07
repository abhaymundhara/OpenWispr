use arboard::{Clipboard, ImageData};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use enigo::{Enigo, Key, KeyboardControllable};
use serde::Serialize;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use stt::{create_adapter, AudioFormat as SttAudioFormat, SttAdapter, SttConfig};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;

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

pub fn start_recording_for_capture(capture: &AudioCapture, app: AppHandle) -> Result<(), String> {
    let mut stream_lock = capture.stream.lock().unwrap();
    if stream_lock.stream.is_some() {
        return Ok(());
    }

    // Get the default audio host
    let host = cpal::default_host();

    // Get the default input device
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;

    // Get the default input config
    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;

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
        if let Some(window) = app.get_window("main") {
            let _ = window.hide();
        }
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

    let audio_seconds = if format.sample_rate > 0 && format.channels > 0 {
        audio_data.len() as f32 / format.sample_rate as f32 / format.channels as f32
    } else {
        0.0
    };
    let model_name = loaded_model_guard
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
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
            if let Some(window) = app.get_window("main") {
                let _ = window.hide();
            }
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
