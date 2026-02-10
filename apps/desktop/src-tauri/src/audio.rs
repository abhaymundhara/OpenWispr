use arboard::{Clipboard, ImageData};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use enigo::{Enigo, Key, KeyboardControllable};
use serde::Serialize;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use stt::{create_adapter, AudioFormat as SttAudioFormat, SttAdapter, SttConfig};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{S_OK, HANDLE},
    Media::Audio::{
        eRender, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator,
        IAudioEndpointVolume,
    },
    System::Com::{
        CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    },
    UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    },
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
    is_recording: Arc<Mutex<bool>>,
    text_processor: Arc<AsyncMutex<Option<text_processor::TextProcessor>>>,
    loaded_llm_model: Arc<AsyncMutex<Option<String>>>,
    was_system_muted: Arc<Mutex<Option<bool>>>,
    is_command_mode: Arc<Mutex<bool>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: Arc::new(Mutex::new(AudioStream { stream: None })),
            samples: Arc::new(Mutex::new(Vec::new())),
            format: Arc::new(Mutex::new(SttAudioFormat::default())),
            stt_adapter: Arc::new(AsyncMutex::new(None)),
            loaded_model: Arc::new(AsyncMutex::new(None)),
            is_recording: Arc::new(Mutex::new(false)),
            text_processor: Arc::new(AsyncMutex::new(None)),
            loaded_llm_model: Arc::new(AsyncMutex::new(None)),
            was_system_muted: Arc::new(Mutex::new(None)),
            is_command_mode: Arc::new(Mutex::new(false)),
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
            is_recording: self.is_recording.clone(),
            text_processor: self.text_processor.clone(),
            loaded_llm_model: self.loaded_llm_model.clone(),
            was_system_muted: self.was_system_muted.clone(),
            is_command_mode: self.is_command_mode.clone(),
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

fn verbose_logs_enabled() -> bool {
    std::env::var("OPENWISPR_VERBOSE_LOGS")
        .ok()
        .as_deref()
        .map(|v| v == "1")
        .unwrap_or(false)
}

enum ClipboardSnapshot {
    Html {
        html: String,
        alt_text: String,
    },
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
    FileList(Vec<PathBuf>),
    Clear,
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
        if verbose_logs_enabled() {
            eprintln!("[paste] could not query frontmost app pid with osascript");
        }
        return;
    };
    if !pid_output.status.success() {
        if verbose_logs_enabled() {
            eprintln!("[paste] osascript query for frontmost app pid failed");
        }
        return;
    }

    let Ok(raw_pid) = String::from_utf8(pid_output.stdout) else {
        return;
    };
    let Some(pid) = parse_frontmost_pid(&raw_pid) else {
        if verbose_logs_enabled() {
            eprintln!("[paste] invalid frontmost app pid '{}'", raw_pid.trim());
        }
        return;
    };
    let self_pid = std::process::id() as i32;
    if pid == self_pid {
        if verbose_logs_enabled() {
            eprintln!(
                "[paste] ignoring self pid {} as paste target (openwispr frontmost)",
                pid
            );
        }
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
        if verbose_logs_enabled() {
            println!("[paste] captured frontmost app pid={} name='{}'", pid, name);
        }
        *slot = Some(MacPasteTarget { pid, name });
    }
}

#[cfg(target_os = "windows")]
fn capture_active_paste_target() {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        if verbose_logs_enabled() {
            eprintln!("[paste] GetForegroundWindow returned null");
        }
        return;
    }
    if let Ok(mut slot) = paste_target_slot().lock() {
        if verbose_logs_enabled() {
            println!("[paste] captured foreground HWND 0x{:X}", hwnd as usize);
        }
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
    let target = paste_target_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    let Some(target) = target else {
        if verbose_logs_enabled() {
            eprintln!("[paste] no captured app to restore on macOS");
        }
        return;
    };

    let result = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to set frontmost of (first application process whose unix id is {}) to true",
            target.pid
        ))
        .status();
    if result.as_ref().is_ok_and(|status| status.success()) && verbose_logs_enabled() {
        println!(
            "[paste] restored focus to app pid={} name='{}'",
            target.pid, target.name
        );
    } else {
        if verbose_logs_enabled() {
            eprintln!(
                "[paste] failed to restore focus to app pid={} name='{}'",
                target.pid, target.name
            );
        }
    }
    thread::sleep(Duration::from_millis(45));
}

#[cfg(target_os = "windows")]
fn restore_active_paste_target() {
    let target = paste_target_slot().lock().ok().and_then(|slot| *slot);
    let Some(hwnd) = target else {
        if verbose_logs_enabled() {
            eprintln!("[paste] no captured HWND to restore on Windows");
        }
        return;
    };

    let valid = unsafe { IsWindow(hwnd) != 0 };
    if !valid {
        if verbose_logs_enabled() {
            eprintln!("[paste] captured HWND is no longer valid");
        }
        return;
    }

    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    if verbose_logs_enabled() {
        println!("[paste] restored focus to HWND 0x{:X}", hwnd as usize);
    }
    thread::sleep(Duration::from_millis(45));
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn restore_active_paste_target() {}

fn capture_clipboard(clipboard: &mut Clipboard) -> ClipboardSnapshot {
    if let Ok(html) = clipboard.get().html() {
        let alt_text = clipboard.get_text().unwrap_or_else(|_| html.clone());
        return ClipboardSnapshot::Html { html, alt_text };
    }

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

    if let Ok(files) = clipboard.get().file_list() {
        return ClipboardSnapshot::FileList(files);
    }

    ClipboardSnapshot::Clear
}

fn restore_clipboard(
    clipboard: &mut Clipboard,
    snapshot: &ClipboardSnapshot,
) -> Result<(), String> {
    match snapshot {
        ClipboardSnapshot::Html { html, alt_text } => clipboard
            .set()
            .html(html.clone(), Some(alt_text.clone()))
            .map_err(|e| format!("failed to restore html clipboard: {}", e)),
        ClipboardSnapshot::Text(text) => clipboard
            .set_text(text.clone())
            .map_err(|e| format!("failed to restore text clipboard: {}", e)),
        ClipboardSnapshot::Image {
            width,
            height,
            bytes,
        } => clipboard
            .set_image(ImageData {
                width: *width,
                height: *height,
                bytes: Cow::Borrowed(bytes.as_slice()),
            })
            .map_err(|e| format!("failed to restore image clipboard: {}", e)),
        ClipboardSnapshot::FileList(paths) => clipboard
            .set()
            .file_list(paths)
            .map_err(|e| format!("failed to restore file list clipboard: {}", e)),
        ClipboardSnapshot::Clear => clipboard
            .clear()
            .map_err(|e| format!("failed to clear clipboard during restore: {}", e)),
    }
}

fn restore_clipboard_with_retry(snapshot: ClipboardSnapshot) {
    for attempt in 1..=10 {
        let mut clipboard = match Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(err) => {
                if verbose_logs_enabled() {
                    eprintln!(
                        "[paste] restore attempt {}: clipboard unavailable: {}",
                        attempt, err
                    );
                }
                thread::sleep(Duration::from_millis(80));
                continue;
            }
        };

        match restore_clipboard(&mut clipboard, &snapshot) {
            Ok(_) => return,
            Err(err) => {
                if verbose_logs_enabled() {
                    eprintln!("[paste] restore attempt {} failed: {}", attempt, err);
                }
            }
        }

        thread::sleep(Duration::from_millis(80));
    }

    if verbose_logs_enabled() {
        eprintln!("[paste] failed to restore clipboard after retries");
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

    // Step 1: stage transcription in system clipboard while OpenWispr is still active.
    let mut clipboard = match Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(err) => {
            if verbose_logs_enabled() {
                eprintln!(
                    "[paste] clipboard unavailable, falling back to direct typing: {}",
                    err
                );
            }
            restore_active_paste_target();
            insert_text_directly(text);
            return Ok(());
        }
    };

    let snapshot = capture_clipboard(&mut clipboard);

    if let Err(err) = clipboard.set_text(text.to_string()) {
        if verbose_logs_enabled() {
            eprintln!("[paste] failed to set clipboard: {}", err);
        }
        restore_active_paste_target();
        insert_text_directly(text);
        return Ok(());
    }

    // Step 2: focus target and paste.
    let mut paste_done = false;
    #[cfg(target_os = "macos")]
    {
        let target = paste_target_slot()
            .lock()
            .ok()
            .and_then(|slot| slot.clone());
        if let Some(target) = target {
            let script = format!(
                r#"tell application "System Events"
    set frontmost of (first application process whose unix id is {}) to true
    delay 0.08
    keystroke "v" using command down
end tell"#,
                target.pid
            );

            let result = Command::new("osascript").arg("-e").arg(&script).status();

            if result.as_ref().is_ok_and(|status| status.success()) {
                paste_done = true;
            } else if verbose_logs_enabled() {
                eprintln!("[paste] osascript focus+paste failed");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        restore_active_paste_target();
        thread::sleep(Duration::from_millis(50));
        trigger_paste_shortcut();
        paste_done = true;
    }

    if !paste_done {
        restore_active_paste_target();
        thread::sleep(Duration::from_millis(50));
        trigger_paste_shortcut();
    }

    // Step 3: restore original clipboard (reliable retries).
    thread::sleep(Duration::from_millis(120));
    restore_clipboard_with_retry(snapshot);

    Ok(())
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn is_system_muted_macos() -> bool {
    let output = Command::new("osascript")
        .arg("-e")
        .arg("output muted of (get volume settings)")
        .output();

    if let Ok(out) = output {
        String::from_utf8_lossy(&out.stdout).trim() == "true"
    } else {
        false
    }
}

fn set_system_muted_macos(muted: bool) {
    let arg = if muted {
        "set volume with output muted"
    } else {
        "set volume without output muted"
    };
    let _ = Command::new("osascript").arg("-e").arg(arg).output();
}

#[cfg(target_os = "windows")]
fn get_master_volume_controls() -> Option<*mut IAudioEndpointVolume> {
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED);
        
        let mut enumerator: *mut IMMDeviceEnumerator = std::ptr::null_mut();
        let hr = windows_sys::Win32::System::Com::CoCreateInstance(
            &MMDeviceEnumerator as *const _ as *const _,
            std::ptr::null_mut(),
            CLSCTX_ALL,
            &windows_sys::Win32::Media::Audio::IMMDeviceEnumerator::IID as *const _ as *const _,
            &mut enumerator as *mut _ as *mut _,
        );
        
        if hr != S_OK { return None; }
        
        let mut device = std::ptr::null_mut();
        let hr = (*enumerator).GetDefaultAudioEndpoint(eRender, eConsole, &mut device);
        if hr != S_OK { return None; }
        
        let mut volume = std::ptr::null_mut();
        let hr = (*device).Activate(
            &windows_sys::Win32::Media::Audio::IAudioEndpointVolume::IID as *const _ as *const _,
            CLSCTX_ALL,
            std::ptr::null_mut(),
            &mut volume as *mut _ as *mut _,
        );
        
        if hr == S_OK {
            Some(volume as *mut IAudioEndpointVolume)
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn is_system_muted_windows() -> bool {
    let mut muted = 0;
    if let Some(volume) = get_master_volume_controls() {
        unsafe {
            (*volume).GetMute(&mut muted);
            CoUninitialize();
        }
    }
    muted != 0
}

#[cfg(target_os = "windows")]
fn set_system_muted_windows(muted: bool) {
    if let Some(volume) = get_master_volume_controls() {
        unsafe {
            (*volume).SetMute(muted as i32, std::ptr::null());
            CoUninitialize();
        }
    }
}

fn mute_system(capture: &AudioCapture) {
    let settings = crate::store::get_settings();
    if !settings.mute_system_audio {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let already_muted = is_system_muted_macos();
        let mut was_muted_guard = capture.was_system_muted.lock().unwrap();
        *was_muted_guard = Some(already_muted);
        if !already_muted {
            set_system_muted_macos(true);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let already_muted = is_system_muted_windows();
        let mut was_muted_guard = capture.was_system_muted.lock().unwrap();
        *was_muted_guard = Some(already_muted);
        if !already_muted {
            set_system_muted_windows(true);
        }
    }
}

fn unmute_system(capture: &AudioCapture) {
    let mut was_muted_guard = capture.was_system_muted.lock().unwrap();
    if let Some(was_muted) = was_muted_guard.take() {
        if !was_muted {
            #[cfg(target_os = "macos")]
            set_system_muted_macos(false);

            #[cfg(target_os = "windows")]
            set_system_muted_windows(false);
        }
    }
}

fn apply_snippets(text: &str, snippets: &[crate::store::Snippet]) -> String {
    let mut result = text.to_string();
    
    // Sort snippets by trigger length descending to avoid partial matches on longer triggers
    let mut sorted_snippets = snippets.to_vec();
    sorted_snippets.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));

    for snippet in sorted_snippets {
        if snippet.trigger.is_empty() { continue; }
        
        // Handle variables in expansion
        let mut expansion = snippet.expansion.clone();
        if expansion.contains("{{date}}") {
            let date = chrono::Local::now().format("%Y-%m-%d").to_string();
            expansion = expansion.replace("{{date}}", &date);
        }
        if expansion.contains("{{time}}") {
            let time = chrono::Local::now().format("%H:%M").to_string();
            expansion = expansion.replace("{{time}}", &time);
        }

        // Case-insensitive replacement for the trigger
        // We use a regex or simple loop to ensure we match whole words if needed, 
        // but for now, let's do simple replace for flexibility.
        let trigger_lower = snippet.trigger.to_lowercase();
        
        // Simple case-insensitive replacement logic
        let mut new_result = String::new();
        let mut last_end = 0;
        let result_lower = result.to_lowercase();
        
        while let Some(start) = result_lower[last_end..].find(&trigger_lower) {
            let abs_start = last_end + start;
            new_result.push_str(&result[last_end..abs_start]);
            new_result.push_str(&expansion);
            last_end = abs_start + trigger_lower.len();
        }
        new_result.push_str(&result[last_end..]);
        result = new_result;
    }
    
    result
}

fn select_input_device(host: &Host, app: &AppHandle) -> Result<Device, String> {
    // 1. Check persistent store
    if let Some(preferred_id) = crate::store::get_input_device_id() {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if let Ok(name) = device.name() {
                    if name == preferred_id {
                        if verbose_logs_enabled() {
                            println!("[audio] selected input device from store: {}", name);
                        }
                        return Ok(device);
                    }
                }
            }
        }
    }

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
                    if verbose_logs_enabled() {
                        println!("[audio] selected input device by env override: {}", name);
                    }
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
        
        // Auto-save this as preferred if none was set
        if crate::store::get_input_device_id().is_none() {
            crate::store::set_input_device_id(app, name.clone());
        }

        if verbose_logs_enabled() {
            println!("[audio] selected hardware default input device: {}", name);
        }
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

    let resolved = ffmpeg_binary_candidates().iter().find_map(|candidate| {
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

pub async fn start_recording_for_capture(capture: &AudioCapture, app: AppHandle, is_command: bool) -> Result<(), String> {
    // Check if we already have a stream
    {
        let stream_lock = capture.stream.lock().unwrap();
        if stream_lock.stream.is_some() {
            return Ok(());
        }
    }

    // Set command mode state
    {
        let mut mode_guard = capture.is_command_mode.lock().unwrap();
        *mode_guard = is_command;
    }
    
    // Emit state to frontend for visual indicator
    let _ = app.emit_all("recording-state", is_command);

    // Get the default audio host
    let host = cpal::default_host();

    // Mute system if enabled
    mute_system(capture);

    // Get the selected input device
    let device = select_input_device(&host, &app)?;

    // Get the default input config
    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;
    
    if verbose_logs_enabled() {
        println!(
            "[audio] input format sample_rate={} channels={} sample_format={:?}",
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        );
    }

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

    // Set recording state
    {
        let mut recording = capture.is_recording.lock().unwrap();
        *recording = true;
    }

    // Build the input stream
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_input_stream::<f32>(&device, &config.into(), app.clone(), capture.samples.clone())?
        }
        cpal::SampleFormat::I16 => {
            build_input_stream::<i16>(&device, &config.into(), app.clone(), capture.samples.clone())?
        }
        cpal::SampleFormat::U16 => {
            build_input_stream::<u16>(&device, &config.into(), app.clone(), capture.samples.clone())?
        }
        _ => return Err("Unsupported sample format".to_string()),
    };

    stream
        .play()
        .map_err(|e| format!("Failed to play stream: {}", e))?;

    // Store the stream and drop the lock immediately
    {
        let mut stream_lock = capture.stream.lock().unwrap();
        stream_lock.stream = Some(stream);
    }

    // Spawn partial transcription loop
    let capture_clone = capture.clone();
    let app_clone = app.clone();
    
    // Use tauri::async_runtime::spawn for consistency
    tauri::async_runtime::spawn(async move {
        // Wait a bit before starting partials
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        while {
            let recording = capture_clone.is_recording.lock().unwrap();
            *recording
        } {
            // Run partial transcription
            if let Err(e) = run_partial_transcription(capture_clone.clone(), app_clone.clone()).await {
                if verbose_logs_enabled() {
                    println!("[stt] partial transcription error: {}", e);
                }
            }
            
            // Wait for next partial
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    });

    Ok(())
}

async fn run_partial_transcription(capture: AudioCapture, app: AppHandle) -> Result<(), String> {
    let samples = {
        let guard = capture.samples.lock().unwrap();
        // At least 2.0s of audio for a meaningful partial
        if guard.len() < (16000.0 * 2.0) as usize {
            return Ok(());
        }
        guard.clone()
    };

    let format = {
        let guard = capture.format.lock().unwrap();
        guard.clone()
    };

    let target_model = crate::models::active_model_value();
    
    // Check if adapter is already loaded for the target model
    let adapter_lock = capture.stt_adapter.lock().await;
    let loaded_model_lock = capture.loaded_model.lock().await;
    
    if adapter_lock.is_none() || loaded_model_lock.as_deref() != Some(target_model.as_str()) {
        return Ok(());
    }

    let adapter = adapter_lock.as_ref().unwrap();
    
    // Run transcription
    let result = adapter.transcribe(&samples, format).await.map_err(|e| e.to_string())?;
    
    if !result.text.trim().is_empty() {
        let _ = app.emit_all(
            "transcription-result",
            TranscriptionResultEvent {
                text: result.text.clone(),
                language: result.language.clone(),
                confidence: result.confidence,
                is_final: false,
            },
        );
    }

    Ok(())
}

#[tauri::command]
pub async fn start_recording(state: tauri::State<'_, AudioCapture>, app: AppHandle) -> Result<(), String> {
    start_recording_for_capture(state.inner(), app, false).await
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
    {
        let mut recording = capture.is_recording.lock().unwrap();
        *recording = false;
    }
    let had_stream = {
        let mut stream_lock = capture.stream.lock().unwrap();
        stream_lock.stream.take().is_some()
    };
    if !had_stream {
        if verbose_logs_enabled() {
            println!("[stt] stop_recording called but no active stream");
        }
        return Ok(());
    }

    if verbose_logs_enabled() {
        println!("[stt] stop_recording: stream stopped, starting transcription");
    }
    emit_transcription_status(&app, "processing", None);

    let audio_data = {
        let mut samples = capture.samples.lock().unwrap();
        std::mem::take(&mut *samples)
    };

    if audio_data.is_empty() {
        return Ok(());
    }

    // Silence detection: prevent phantom transcriptions from background noise
    let rms = calculate_rms(&audio_data);
    if rms < 0.003 {
        if verbose_logs_enabled() {
            println!("[audio] skipping near-silent recording (rms: {:.6})", rms);
        }
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
        if verbose_logs_enabled() {
            println!("[stt] initializing adapter for model: {}", target_model);
        }
        let mut adapter = create_adapter().map_err(|e| {
            let err_msg = format!("Failed to create adapter: {}", e);
            eprintln!("{}", err_msg);
            err_msg
        })?;
        adapter
            .initialize(SttConfig {
                model_name: target_model.clone(),
                ..Default::default()
            })
            .await
            .map_err(|e| {
                let err_msg = format!("Failed to initialize adapter: {}", e);
                eprintln!("{}", err_msg);
                // Clean up on initialization failure
                *adapter_guard = None;
                *loaded_model_guard = None;
                err_msg
            })?;
        *adapter_guard = Some(adapter);
        *loaded_model_guard = Some(target_model.clone());
        if verbose_logs_enabled() {
            println!(
                "[stt] adapter initialized successfully for model: {}",
                target_model
            );
        }
    } else if verbose_logs_enabled() {
        println!("[stt] reusing existing adapter for model: {}", target_model);
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
            if verbose_logs_enabled() {
                println!(
                    "[stt] ffmpeg normalization applied samples={} sample_rate={} channels={}",
                    samples.len(),
                    normalized_format.sample_rate,
                    normalized_format.channels
                );
            }
            (samples, normalized_format)
        }
        Err(err) => {
            if verbose_logs_enabled() {
                eprintln!(
                    "[stt] ffmpeg normalization unavailable, using raw capture: {}",
                    err
                );
            }
            (audio_data, format)
        }
    };
    let audio_seconds = if format.sample_rate > 0 && format.channels > 0 {
        audio_data.len() as f32 / format.sample_rate as f32 / format.channels as f32
    } else {
        0.0
    };
    if verbose_logs_enabled() {
        println!(
            "[stt] transcription started model={} samples={} duration_s={:.2} sample_rate={} channels={}",
            model_name,
            audio_data.len(),
            audio_seconds,
            format.sample_rate,
            format.channels
        );
    }

    match adapter.transcribe(&audio_data, format).await {
        Ok(result) => {
            let language = result
                .language
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            if verbose_logs_enabled() {
                println!(
                    "[stt] transcription complete model={} language={} chars={}",
                    model_name,
                    language,
                    result.text.chars().count()
                );
            }
            println!("[stt] raw: {}", result.text.trim());

            // Update stats
            let duration = audio_seconds as f64;
            let word_count = result.text.split_whitespace().count() as u64;
            crate::store::update_analytics(&app, duration, word_count);

            // Get formatting settings
            let settings = crate::store::get_settings();
            let mut final_text = result.text.clone();

            // Apply text formatting if enabled
            if settings.text_formatting_enabled {
                if verbose_logs_enabled() {
                    println!(
                        "[formatting] enabled with core logic"
                    );
                }

                // Get active model for formatting
                let format_model = settings
                    .system_llm_model
                    .unwrap_or_else(|| "SmolLM2-135M-Instruct-Q4_K_M".to_string());

                let transcribed_text = result.text.clone();
                let personal_dictionary = settings.personal_dictionary.clone();
                let processor_cache = capture.text_processor.clone();
                let loaded_llm_cache = capture.loaded_llm_model.clone();

                match tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        use text_processor::TextProcessor;

                        let mut proc_guard = processor_cache.lock().await;
                        let mut loaded_guard = loaded_llm_cache.lock().await;

                        if proc_guard.is_none() || loaded_guard.as_deref() != Some(format_model.as_str()) {
                            if verbose_logs_enabled() {
                                println!("[formatting] initializing processor for model: {}", format_model);
                            }
                            let processor = TextProcessor::new(&format_model).await?;
                            *proc_guard = Some(processor);
                            *loaded_guard = Some(format_model.clone());
                        }

                        let processor = proc_guard.as_ref().unwrap();
                        let mode = if *capture.is_command_mode.lock().unwrap() {
                            settings.text_formatting_mode.clone()
                        } else {
                            "smart".to_string()
                        };
                        processor.process(&transcribed_text, &personal_dictionary, &mode).await
                    })
                }) {
                    Ok(processing_result) => {
                        final_text = processing_result.formatted_text;
                        
                        // Apply snippets after formatting
                        if !settings.snippets.is_empty() {
                            final_text = apply_snippets(&final_text, &settings.snippets);
                        }

                        println!("[formatting] final: {}", final_text.trim());
                    }
                    Err(e) => {
                        eprintln!("[formatting] failed, using raw text: {}", e);
                        // Fallback to original text - already set in final_text
                    }
                }
            }

            // Paste synchronously BEFORE emitting events to ensure it completes
            if verbose_logs_enabled() {
                println!(
                    "[paste] attempting to paste {} chars to active window",
                    final_text.chars().count()
                );
            }

            if let Err(err) = paste_text_preserving_clipboard(&final_text) {
                eprintln!("[paste] ERROR failed to paste text: {}", err);
            } else {
                println!("[paste] text pasted to active window");
            }

            // Wait for paste to physically complete (osascript has 80ms delay on macOS)
            // This ensures the text is actually typed before we emit "idle"
            #[cfg(target_os = "macos")]
            std::thread::sleep(std::time::Duration::from_millis(150));
            
            #[cfg(not(target_os = "macos"))]
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Now emit result to UI
            let _ = app.emit_all(
                "transcription-result",
                TranscriptionResultEvent {
                    text: result.text.clone(),
                    language: result.language.clone(),
                    confidence: result.confidence,
                    is_final: true,
                },
            );

            // Set idle status AFTER paste is complete
            emit_transcription_status(&app, "idle", None);

            // Restore system volume
            unmute_system(&capture);

            if verbose_logs_enabled() {
                if verbose_logs_enabled() {
                    println!("[stt] transcription cycle complete, ready for next run");
                }
            }

            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            eprintln!("[stt] transcription failed: {}", message);
            emit_transcription_status(&app, "error", Some(message.clone()));
            // Stay in error state - don't emit idle to avoid pill flickering
            // The next dictation cycle will reset to listening state
            println!("[stt] error reported, adapter still loaded for next run");

            // Restore system volume even on error
            unmute_system(&capture);

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

#[derive(Serialize)]
pub struct AudioDevice {
    id: String,
    name: String,
}

#[tauri::command]
pub fn list_input_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            result.push(AudioDevice {
                id: name.clone(),
                name,
            });
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn set_input_device(app: AppHandle, device_id: String) -> Result<(), String> {
    crate::store::set_input_device_id(&app, device_id);
    Ok(())
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
