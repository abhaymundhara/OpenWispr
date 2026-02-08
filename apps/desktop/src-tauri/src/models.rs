use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use stt::{create_adapter, SttConfig};

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

    #[cfg(target_os = "macos")]
    {
        let mlx_models = [
            "mlx-community/whisper-tiny",
            "mlx-community/whisper-base",
            "mlx-community/whisper-small",
            "mlx-community/whisper-medium",
            "mlx-community/whisper-large-v3-turbo",
            "mlx-community/whisper-large-v3",
        ];

        for model in mlx_models {
            result.push(ModelInfo {
                name: model.to_string(),
                runtime: "mlx-whisper".to_string(),
                downloaded: false,
                can_download: false,
                note: Some("MLX runtime in progress".to_string()),
            });
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn download_model(model: String) -> Result<(), String> {
    eprintln!("🚀 download_model command called for: {}", model);
    
    if model.starts_with("mlx-community/") {
        return Err("MLX model downloads are not enabled yet".to_string());
    }

    eprintln!("📦 Creating STT adapter...");
    let mut adapter = create_adapter().map_err(|e| {
        eprintln!("❌ Failed to create adapter: {}", e);
        e.to_string()
    })?;
    
    eprintln!("🔧 Initializing adapter with model: {}", model);
    adapter
        .initialize(SttConfig {
            model_name: model.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| {
            eprintln!("❌ Failed to initialize adapter: {}", e);
            e.to_string()
        })?;
    
    eprintln!("✅ Model {} ready!", model);
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
    if model.starts_with("mlx-community/") {
        return Err("MLX model selection is not enabled yet".to_string());
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
