use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use stt::{create_adapter, AudioFormat as SttAudioFormat, SttConfig};
use std::sync::{Arc, Mutex};

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
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: Arc::new(Mutex::new(AudioStream { stream: None })),
            samples: Arc::new(Mutex::new(Vec::new())),
            format: Arc::new(Mutex::new(SttAudioFormat::default())),
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

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

#[tauri::command]
pub fn start_recording(
    state: tauri::State<AudioCapture>,
    app: AppHandle,
) -> Result<(), String> {
    let mut stream_lock = state.stream.lock().unwrap();
    
    // Stop existing stream if any
    stream_lock.stream = None;
    
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
        let mut samples = state.samples.lock().unwrap();
        samples.clear();
    }
    {
        let mut format = state.format.lock().unwrap();
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
            build_input_stream::<f32>(&device, &config.into(), app, state.samples.clone())?
        }
        cpal::SampleFormat::I16 => {
            build_input_stream::<i16>(&device, &config.into(), app, state.samples.clone())?
        }
        cpal::SampleFormat::U16 => {
            build_input_stream::<u16>(&device, &config.into(), app, state.samples.clone())?
        }
        _ => return Err("Unsupported sample format".to_string()),
    };
    
    stream
        .play()
        .map_err(|e| format!("Failed to play stream: {}", e))?;
    
    stream_lock.stream = Some(stream);
    
    Ok(())
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
                let samples: Vec<f32> = data.iter().map(|&s| cpal::Sample::from_sample(s)).collect();
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

#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, AudioCapture>,
    app: AppHandle,
) -> Result<(), String> {
    {
        let mut stream_lock = state.stream.lock().unwrap();
        stream_lock.stream = None;
    }

    emit_transcription_status(&app, "processing", None);

    let audio_data = {
        let mut samples = state.samples.lock().unwrap();
        std::mem::take(&mut *samples)
    };

    if audio_data.is_empty() {
        emit_transcription_status(&app, "idle", None);
        return Ok(());
    }

    let format = {
        let format = state.format.lock().unwrap();
        format.clone()
    };
    let mut adapter = create_adapter().map_err(|e| e.to_string())?;
    adapter
        .initialize(SttConfig::default())
        .await
        .map_err(|e| e.to_string())?;

    match adapter.transcribe(&audio_data, format).await {
        Ok(result) => {
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
            emit_transcription_status(&app, "error", Some(message.clone()));
            Err(message)
        }
    }
}
