use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use stt::{create_adapter, SttConfig};

const SHERPA_PARAKEET_MODEL: &str = "parakeet-tdt-0.6b-v2";

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub runtime: String,
    pub downloaded: bool,
    pub can_download: bool,
    pub note: Option<String>,
}

fn active_model_store() -> &'static Mutex<String> {
    static ACTIVE_MODEL: OnceLock<Mutex<String>> = OnceLock::new();
    ACTIVE_MODEL.get_or_init(|| Mutex::new("base".to_string()))
}

pub fn active_model_value() -> String {
    active_model_store()
        .lock()
        .map(|v| v.clone())
        .unwrap_or_else(|_| "base".to_string())
}

fn is_sherpa_model(model: &str) -> bool {
    model == SHERPA_PARAKEET_MODEL
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<ModelInfo>, String> {
    let adapter = create_adapter().map_err(|e| e.to_string())?;
    let mut result = Vec::new();

    for name in adapter.available_models() {
        let downloaded = adapter.is_model_available(&name).await;
        result.push(ModelInfo {
            name,
            runtime: "whisper.cpp".to_string(),
            downloaded,
            can_download: true,
            note: None,
        });
    }

    result.push(ModelInfo {
        name: SHERPA_PARAKEET_MODEL.to_string(),
        runtime: "sherpa-onnx".to_string(),
        downloaded: false,
        can_download: false,
        note: Some("Alternative runtime option (integration pending)".to_string()),
    });

    Ok(result)
}

#[tauri::command]
pub async fn download_model(model: String) -> Result<(), String> {
    if is_sherpa_model(&model) {
        return Err("Sherpa ONNX model downloads are not enabled yet".to_string());
    }

    let mut adapter = create_adapter().map_err(|e| e.to_string())?;
    adapter
        .initialize(SttConfig {
            model_name: model,
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_active_model() -> Result<String, String> {
    active_model_store()
        .lock()
        .map(|v| v.clone())
        .map_err(|_| "failed to read active model".to_string())
}

#[tauri::command]
pub async fn set_active_model(model: String) -> Result<(), String> {
    if is_sherpa_model(&model) {
        return Err("Sherpa ONNX runtime selection is not enabled yet".to_string());
    }

    let adapter = create_adapter().map_err(|e| e.to_string())?;
    let downloaded = adapter.is_model_available(&model).await;
    if !downloaded {
        return Err("Model must be downloaded before selection".to_string());
    }

    let mut guard = active_model_store()
        .lock()
        .map_err(|_| "failed to set active model".to_string())?;
    *guard = model;
    Ok(())
}
