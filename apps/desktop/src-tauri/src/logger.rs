use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use chrono::{DateTime, Local};

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

fn get_log_path() -> PathBuf {
    let mut path = PathBuf::from("E:\\OpenWispr");
    path.push("logs");
    std::fs::create_dir_all(&path).ok();
    
    // Create log file with timestamp for each run
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    path.push(format!("openwispr-{}.log", timestamp));
    path
}

fn ensure_log_file() -> Option<File> {
    let mut guard = LOG_FILE.lock().ok()?;
    if guard.is_none() {
        let log_path = get_log_path();
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                eprintln!("📝 Log file: {}", log_path.display());
                *guard = Some(file);
            }
            Err(e) => {
                eprintln!("❌ Failed to open log file: {}", e);
                return None;
            }
        }
    }
    None // Return after ensuring file is initialized
}

pub fn log_info(message: &str) {
    ensure_log_file();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_line = format!("[{}] INFO: {}\n", timestamp, message);
    
    eprintln!("{}", log_line.trim());
    
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

pub fn log_error(message: &str) {
    ensure_log_file();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_line = format!("[{}] ERROR: {}\n", timestamp, message);
    
    eprintln!("{}", log_line.trim());
    
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

pub fn log_session_start() {
    ensure_log_file();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let separator = "=".repeat(80);
    let log_line = format!("\n{}\n[{}] SESSION START\n{}\n", separator, timestamp, separator);
    
    eprintln!("{}", log_line.trim());
    
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

#[derive(Clone)]
pub struct SessionLogger {
    session_id: String,
    start_time: DateTime<Local>,
}

impl SessionLogger {
    pub fn new() -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let start_time = Local::now();
        
        log_info(&format!("🎙️  Dictation session started: {}", session_id));
        
        Self {
            session_id,
            start_time,
        }
    }
    
    pub fn log_audio_capture(&self, samples: usize, duration_secs: f32, sample_rate: u32, channels: u16) {
        log_info(&format!(
            "[{}] Audio captured: {} samples, {:.2}s duration, {}Hz, {} channels",
            self.session_id, samples, duration_secs, sample_rate, channels
        ));
    }
    
    pub fn log_audio_empty(&self) {
        log_error(&format!(
            "[{}] Audio capture failed: No samples recorded",
            self.session_id
        ));
    }
    
    pub fn log_model_load(&self, model_name: &str) {
        log_info(&format!(
            "[{}] Model loaded: {}",
            self.session_id, model_name
        ));
    }
    
    pub fn log_transcription_start(&self, model_name: &str) {
        log_info(&format!(
            "[{}] Transcription started with model: {}",
            self.session_id, model_name
        ));
    }
    
    pub fn log_transcription_success(&self, text: &str, language: Option<&str>, confidence: Option<f32>) {
        let elapsed = Local::now().signed_duration_since(self.start_time);
        log_info(&format!(
            "[{}] Transcription completed in {:.2}s | Language: {} | Confidence: {} | Text length: {} chars",
            self.session_id,
            elapsed.num_milliseconds() as f32 / 1000.0,
            language.unwrap_or("unknown"),
            confidence.map(|c| format!("{:.2}", c)).unwrap_or_else(|| "n/a".to_string()),
            text.len()
        ));
        log_info(&format!("[{}] Transcribed text: \"{}\"", self.session_id, text));
    }
    
    pub fn log_transcription_error(&self, error: &str) {
        let elapsed = Local::now().signed_duration_since(self.start_time);
        log_error(&format!(
            "[{}] Transcription failed after {:.2}s: {}",
            self.session_id,
            elapsed.num_milliseconds() as f32 / 1000.0,
            error
        ));
    }
    
    pub fn log_paste_success(&self, target_info: &str) {
        log_info(&format!(
            "[{}] Text pasted successfully to: {}",
            self.session_id, target_info
        ));
    }
    
    pub fn log_paste_error(&self, error: &str) {
        log_error(&format!(
            "[{}] Paste failed: {}",
            self.session_id, error
        ));
    }
    
    pub fn log_session_complete(&self) {
        let elapsed = Local::now().signed_duration_since(self.start_time);
        log_info(&format!(
            "[{}] Session completed in {:.2}s",
            self.session_id,
            elapsed.num_milliseconds() as f32 / 1000.0
        ));
    }
}
