use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock};
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
                    error: Some(error.clone()),
                    message: Some("Download failed".to_string()),
                },
            );
            Err(error)
        }
    }
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
