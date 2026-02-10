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

fn to_user_facing_llm_download_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return "LLM model download timed out. Please retry.".to_string();
    }
    if lower.contains("network") || lower.contains("dns") || lower.contains("resolve") {
        return "Network error while downloading LLM model.".to_string();
    }
    if lower.contains("no space") || lower.contains("disk full") {
        return "Not enough disk space for the selected LLM model.".to_string();
    }
    format!("LLM download failed: {}", raw)
}

#[tauri::command]
pub fn list_llm_models() -> Result<Vec<LlmModelInfo>, String> {
    Ok(models::list_models())
}

#[tauri::command]
pub async fn download_llm_model(app: AppHandle, model: String) -> Result<(), String> {
    let model_clone = model.clone();
    let app_clone = app.clone();
    
    let result = models::download_model(&model, Some(Box::new(move |downloaded, total| {
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
    .await;

    if let Err(err) = result {
        let friendly = to_user_facing_llm_download_error(&err.to_string());
        let _ = app.emit_all(
            "llm-model-download-progress",
            ModelDownloadProgressEvent {
                model: model.clone(),
                stage: "error".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                percent: None,
                done: true,
                error: Some(friendly.clone()),
                message: Some("Download failed".to_string()),
            },
        );
        return Err(friendly);
    }

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
