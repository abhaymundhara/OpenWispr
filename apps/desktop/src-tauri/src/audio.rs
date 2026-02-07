use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use std::sync::Mutex;

// Simple wrapper to make Stream thread-safe
struct AudioStream {
    stream: Option<Stream>,
}

unsafe impl Send for AudioStream {}
unsafe impl Sync for AudioStream {}

pub struct AudioCapture {
    stream: Arc<Mutex<AudioStream>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: Arc::new(Mutex::new(AudioStream { stream: None })),
        }
    }
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
    
    println!("Using audio config: {:?}", config);
    
    // Build the input stream
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &config.into(), app)?,
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &config.into(), app)?,
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &config.into(), app)?,
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
pub fn stop_recording(state: tauri::State<AudioCapture>) -> Result<(), String> {
    let mut stream_lock = state.stream.lock().unwrap();
    stream_lock.stream = None;
    Ok(())
}
