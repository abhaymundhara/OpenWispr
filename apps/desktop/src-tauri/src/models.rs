use serde::Serialize;
use std::sync::Arc;
use stt::{
    create_adapter, is_mlx_model_name, is_sherpa_model_name, set_model_download_progress_handler,
    ModelDownloadProgress, SttConfig, MLX_PARAKEET_V2_MODEL, SHERPA_PARAKEET_INT8_MODEL,
};
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub runtime: String,
    pub downloaded: bool,
    pub can_download: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ModelDownloadProgressEvent {
    model: String,
    stage: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    percent: Option<f32>,
    done: bool,
    error: Option<String>,
    message: Option<String>,
}

fn emit_model_download_progress_event(app: &tauri::AppHandle, payload: ModelDownloadProgressEvent) {
    let _ = app.emit_all("model-download-progress", payload);
}

fn to_user_facing_download_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return "Download timed out. Please check your connection and retry.".to_string();
    }
    if lower.contains("dns") || lower.contains("resolve") || lower.contains("network") {
        return "Network error while downloading model. Check internet access and retry.".to_string();
    }
    if lower.contains("permission denied") || lower.contains("access is denied") {
        return "OpenWispr cannot write model files. Check folder permissions.".to_string();
    }
    if lower.contains("no space") || lower.contains("disk full") {
        return "Not enough disk space to download this model.".to_string();
    }
    if lower.contains("404") || lower.contains("not found") {
        return "Model artifact not found on the remote host. Try again later.".to_string();
    }
    format!("Model download failed: {}", raw)
}

pub fn active_model_value() -> String {
    crate::store::get_store().settings.active_transcription_model
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
        note: Some("NVIDIA Parakeet TDT v3 int8".to_string()),
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
pub async fn download_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let model_for_callback = model.clone();
    let app_for_callback = app.clone();
    set_model_download_progress_handler(Some(Arc::new(move |progress: ModelDownloadProgress| {
        if progress.model_name != model_for_callback {
            return;
        }
        emit_model_download_progress_event(
            &app_for_callback,
            ModelDownloadProgressEvent {
                model: progress.model_name,
                stage: progress.stage,
                downloaded_bytes: progress.downloaded_bytes,
                total_bytes: progress.total_bytes,
                percent: progress.percent,
                done: progress.done,
                error: progress.error,
                message: progress.message,
            },
        );
    })));

    emit_model_download_progress_event(
        &app,
        ModelDownloadProgressEvent {
            model: model.clone(),
            stage: "queued".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(0.0),
            done: false,
            error: None,
            message: Some("Queued for download".to_string()),
        },
    );

    let mut adapter = create_adapter().map_err(|e| e.to_string())?;
    let result = adapter
        .initialize(SttConfig {
            model_name: model.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string());

    set_model_download_progress_handler(None);

    match result {
        Ok(_) => {
            emit_model_download_progress_event(
                &app,
                ModelDownloadProgressEvent {
                    model,
                    stage: "ready".to_string(),
                    downloaded_bytes: 0,
                    total_bytes: None,
                    percent: Some(100.0),
                    done: true,
                    error: None,
                    message: Some("Download complete".to_string()),
                },
            );
            Ok(())
        }
        Err(error) => {
            let friendly_error = to_user_facing_download_error(&error);
            let downloaded = create_adapter()
                .map_err(|e| e.to_string())?
                .is_model_available(&model)
                .await;
            if downloaded {
                emit_model_download_progress_event(
                    &app,
                    ModelDownloadProgressEvent {
                        model,
                        stage: "ready".to_string(),
                        downloaded_bytes: 0,
                        total_bytes: None,
                        percent: Some(100.0),
                        done: true,
                        error: None,
                        message: Some(
                            "Model files downloaded. It will initialize when selected.".to_string(),
                        ),
                    },
                );
                return Ok(());
            }

            emit_model_download_progress_event(
                &app,
                ModelDownloadProgressEvent {
                    model,
                    stage: "error".to_string(),
                    downloaded_bytes: 0,
                    total_bytes: None,
                    percent: None,
                    done: true,
                    error: Some(friendly_error.clone()),
                    message: Some("Download failed".to_string()),
                },
            );
            Err(friendly_error)
        }
    }
}

#[tauri::command]
pub fn get_active_model() -> Result<String, String> {
    Ok(active_model_value())
}

#[tauri::command]
pub async fn set_active_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let adapter = create_adapter().map_err(|e| e.to_string())?;
    let downloaded = adapter.is_model_available(&model).await;
    if !downloaded {
        return Err("Model must be downloaded before selection".to_string());
    }

    #[cfg(not(target_os = "macos"))]
    if is_mlx_model_name(&model) {
        return Err("MLX models are only supported on macOS".to_string());
    }

    let mut store = crate::store::get_store();
    store.settings.active_transcription_model = model;
    crate::store::save_store(&app, &store);
    Ok(())
}

#[tauri::command]
pub async fn delete_model(model: String) -> Result<(), String> {
    // Prevent deleting the active model
    let active = active_model_value();
    if active == model {
        return Err("Cannot delete the currently active model. Please select a different model first.".to_string());
    }

    let adapter = create_adapter().map_err(|e| e.to_string())?;
    
    // Check if model exists
    let downloaded = adapter.is_model_available(&model).await;
    if !downloaded {
        return Err("Model is not downloaded".to_string());
    }

    // Get model cache directory
    let model_dir = stt::get_model_cache_dir().map_err(|e| e.to_string())?;
    
    // Determine the model file path based on model type
    let model_path = if is_sherpa_model_name(&model) {
        // Sherpa models are in a subdirectory
        model_dir.join(model)
    } else if is_mlx_model_name(&model) {
        // MLX models are in a subdirectory
        model_dir.join(model)
    } else {
        // Whisper models are .bin files
        model_dir.join(format!("ggml-{}.bin", model))
    };

    // Delete the model file(s)
    if model_path.is_dir() {
        std::fs::remove_dir_all(&model_path)
            .map_err(|e| format!("Failed to delete model directory: {}", e))?;
    } else if model_path.is_file() {
        std::fs::remove_file(&model_path)
            .map_err(|e| format!("Failed to delete model file: {}", e))?;
    } else {
        return Err("Model file not found".to_string());
    }

    Ok(())
}
