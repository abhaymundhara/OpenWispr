use llm::{models, LlmModelInfo};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct ModelDownloadProgressEvent {
    pub model: String,
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f32>,
    pub done: bool,
    pub error: Option<String>,
    pub message: Option<String>,
}

#[tauri::command]
pub fn list_llm_models() -> Result<Vec<LlmModelInfo>, String> {
    Ok(models::list_models())
}

#[tauri::command]
pub async fn download_llm_model(app: AppHandle, model: String) -> Result<(), String> {
    let model_clone = model.clone();
    let app_clone = app.clone();
    
    models::download_model(&model, Some(Box::new(move |downloaded, total| {
        let percent = if total > 0 {
            (downloaded as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        
        let _ = app_clone.emit_all(
            "llm-model-download-progress",
            ModelDownloadProgressEvent {
                model: model_clone.clone(),
                stage: "downloading".to_string(),
                downloaded_bytes: downloaded,
                total_bytes: Some(total),
                percent: Some(percent),
                done: false,
                error: None,
                message: None,
            },
        );
    })))
    .await
    .map_err(|e| e.to_string())?;

    // Emit completion event
    let _ = app.emit_all(
        "llm-model-download-progress",
        ModelDownloadProgressEvent {
            model: model.clone(),
            stage: "complete".to_string(),
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

#[tauri::command]
pub fn get_active_llm_model() -> Result<String, String> {
    crate::store::get_system_llm_model()
        .ok_or_else(|| "No active LLM model".to_string())
}

#[tauri::command]
pub fn set_active_llm_model(app: AppHandle, model: String) -> Result<(), String> {
    crate::store::set_system_llm_model(&app, model);
    Ok(())
}
