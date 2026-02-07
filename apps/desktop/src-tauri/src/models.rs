use serde::Serialize;
use stt::{create_adapter, SttConfig};

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub downloaded: bool,
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<ModelInfo>, String> {
    let adapter = create_adapter().map_err(|e| e.to_string())?;
    let mut result = Vec::new();

    for name in adapter.available_models() {
        let downloaded = adapter.is_model_available(&name).await;
        result.push(ModelInfo { name, downloaded });
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
