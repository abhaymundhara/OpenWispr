use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Analytics {
    pub lifetime_removed_sec: f64, // "lifetime saved"
    pub sessions_count: u64,
    pub day_streak: u64,
    pub last_session_date: Option<String>, // YYYY-MM-DD
    pub total_words: u64,
    pub total_seconds: f64,
}

impl Default for Analytics {
    fn default() -> Self {
        Self {
            lifetime_removed_sec: 0.0,
            sessions_count: 0,
            day_streak: 0,
            last_session_date: None,
            total_words: 0,
            total_seconds: 0.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub input_device: Option<String>,
    pub language: Option<String>,
    pub local_transcription_enabled: bool,
    // LLM Settings
    pub llm_provider: Option<String>, // "ollama"
    pub ollama_base_url: Option<String>,
    pub ollama_model: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input_device: None,
            language: Some("en".to_string()),
            local_transcription_enabled: true,
            llm_provider: Some("ollama".to_string()),
            ollama_base_url: Some("http://localhost:11434".to_string()),
            ollama_model: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct AppStore {
    pub analytics: Analytics,
    pub settings: Settings,
}

fn store_path(app: &AppHandle) -> Option<PathBuf> {
    app.path_resolver()
        .app_data_dir()
        .map(|dir| dir.join("store.json"))
}

static STORE: OnceLock<Mutex<AppStore>> = OnceLock::new();

pub fn init_store(app: &AppHandle) {
    let path = store_path(app);
    let store = if let Some(path) = &path {
        if path.exists() {
            fs::read_to_string(path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            AppStore::default()
        }
    } else {
        AppStore::default()
    };
    
    let _ = STORE.set(Mutex::new(store));
}

pub fn get_store() -> AppStore {
    STORE.get()
        .map(|s| s.lock().unwrap().clone())
        .unwrap_or_default()
}

pub fn save_store(app: &AppHandle, store: &AppStore) {
    if let Some(path) = store_path(app) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, serde_json::to_string_pretty(store).unwrap_or_default());
    }
    
    // Update memory
    if let Some(guard) = STORE.get() {
        if let Ok(mut lock) = guard.lock() {
            *lock = store.clone();
        }
    }
}

pub fn update_analytics(app: &AppHandle, duration_sec: f64, word_count: u64) {
    let mut store = get_store();
    
    // Update totals
    store.analytics.total_words += word_count;
    store.analytics.total_seconds += duration_sec;
    store.analytics.lifetime_removed_sec += duration_sec * 3.0; // Assuming 3x speedup vs typing? Or just raw time? Let's say raw time for now or a multipliers. User UI says "lifetime saved", usually typing speed vs speaking. Avg speaking 150wpm, typing 40wpm. So ~3-4x.
    store.analytics.sessions_count += 1;

    // Update streak
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if let Some(last) = &store.analytics.last_session_date {
        if last != &today {
            // If last was yesterday, increment. 
            // Simplified: just check if it's a new day. Real streak requires checking consecutive days.
            // For MVP, if last != today, we check if it was yesterday.
            let last_date = chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d").ok();
            let today_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").ok();
            
            if let (Some(l), Some(t)) = (last_date, today_date) {
                if t.signed_duration_since(l).num_days() == 1 {
                    store.analytics.day_streak += 1;
                } else if t.signed_duration_since(l).num_days() > 1 {
                    store.analytics.day_streak = 1;
                }
                // If 0 days (same day), do nothing to streak
            } else {
                 store.analytics.day_streak = 1;
            }
        }
    } else {
        store.analytics.day_streak = 1;
    }
    store.analytics.last_session_date = Some(today);
    
    save_store(app, &store);
    let _ = app.emit_all("analytics-update", &store.analytics);
}

pub fn get_input_device_id() -> Option<String> {
    get_store().settings.input_device
}

pub fn set_input_device_id(app: &AppHandle, device_id: String) {
    let mut store = get_store();
    store.settings.input_device = Some(device_id);
    save_store(app, &store);
}

#[tauri::command]
pub fn get_analytics_stats() -> Analytics {
    get_store().analytics
}

#[tauri::command]
pub fn set_transcription_enabled(app: AppHandle, enabled: bool) {
    let mut store = get_store();
    store.settings.local_transcription_enabled = enabled;
    save_store(&app, &store);
}

#[tauri::command]
pub fn set_language(app: AppHandle, language: String) {
    let mut store = get_store();
    store.settings.language = Some(language);
    save_store(&app, &store);
}

#[tauri::command]
pub fn set_llm_settings(
    app: AppHandle,
    provider: String,
    base_url: String,
    model: String,
) {
    let mut store = get_store();
    store.settings.llm_provider = Some(provider);
    store.settings.ollama_base_url = Some(base_url);
    store.settings.ollama_model = Some(model);
    save_store(&app, &store);
}

#[tauri::command]
pub fn get_settings() -> Settings {
    get_store().settings
}
