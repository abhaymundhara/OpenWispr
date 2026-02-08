use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use stt::{
    create_adapter, is_mlx_model_name, is_sherpa_model_name, SttConfig, MLX_PARAKEET_V2_MODEL,
    SHERPA_PARAKEET_INT8_MODEL,
};

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

    let sherpa_downloaded = adapter.is_model_available(SHERPA_PARAKEET_INT8_MODEL).await;
    result.push(ModelInfo {
        name: SHERPA_PARAKEET_INT8_MODEL.to_string(),
        runtime: "sherpa-onnx".to_string(),
        downloaded: sherpa_downloaded,
        can_download: true,
        note: Some("NVIDIA Parakeet TDT v2 int8".to_string()),
    });

    #[cfg(target_os = "macos")]
    {
        let mlx_downloaded = adapter.is_model_available(MLX_PARAKEET_V2_MODEL).await;
        result.push(ModelInfo {
            name: MLX_PARAKEET_V2_MODEL.to_string(),
            runtime: "mlx-parakeet".to_string(),
            downloaded: mlx_downloaded,
            can_download: true,
            note: Some("Parakeet MLX community model".to_string()),
        });
    }

    Ok(result)
}

#[tauri::command]
pub async fn download_model(model: String) -> Result<(), String> {
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
    let adapter = create_adapter().map_err(|e| e.to_string())?;
    let downloaded = adapter.is_model_available(&model).await;
    if !downloaded {
        return Err("Model must be downloaded before selection".to_string());
    }

    #[cfg(not(target_os = "macos"))]
    if is_mlx_model_name(&model) {
        return Err("MLX models are only supported on macOS".to_string());
    }

    if is_sherpa_model_name(&model) || is_mlx_model_name(&model) {
        // Valid special runtimes with download support.
    }

    let mut guard = active_model_store()
        .lock()
        .map_err(|_| "failed to set active model".to_string())?;
    *guard = model;
    Ok(())
}
