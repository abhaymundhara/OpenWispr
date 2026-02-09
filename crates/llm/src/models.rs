use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    
    #[error("Model not found: {0}")]
    NotFound(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelInfo {
    pub name: String,
    pub size_mb: u64,
    pub downloaded: bool,
    pub description: String,
    pub hf_repo: String,
    pub filename: String,
}

/// Available SmolLM2 models with GGUF quantization
pub const AVAILABLE_MODELS: &[(&str, &str, &str, u64)] = &[
    (
        "SmolLM2-135M-Instruct-Q4_K_M",
        "bartowski/SmolLM2-135M-Instruct-GGUF",
        "SmolLM2-135M-Instruct-Q4_K_M.gguf",
        80, // ~80 MB
    ),
    (
        "SmolLM2-360M-Instruct-Q4_K_M",
        "bartowski/SmolLM2-360M-Instruct-GGUF",
        "SmolLM2-360M-Instruct-Q4_K_M.gguf",
        200, // ~200 MB
    ),
    (
        "SmolLM2-1.7B-Instruct-Q5_K_M",
        "bartowski/SmolLM2-1.7B-Instruct-GGUF",
        "SmolLM2-1.7B-Instruct-Q5_K_M.gguf",
        1200, // ~1.2 GB
    ),
];

/// Get the local model cache directory
pub fn get_model_cache_dir() -> Result<PathBuf> {
    let cache_dir = if let Ok(custom_dir) = std::env::var("OPENWISPR_LLM_MODEL_DIR") {
        PathBuf::from(custom_dir)
    } else {
        #[cfg(target_os = "macos")]
        {
            dirs::home_dir()
                .ok_or_else(|| ModelError::NotFound("Home directory not found".to_string()))?
                .join(".cache/openwispr/llm-models")
        }
        #[cfg(target_os = "windows")]
        {
            dirs::data_local_dir()
                .ok_or_else(|| ModelError::NotFound("Local data directory not found".to_string()))?
                .join("OpenWispr")
                .join("llm-models")
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            return Err(ModelError::NotFound("Unsupported platform".to_string()));
        }
    };

    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

/// Get the path to a specific model file
pub fn get_model_path(model_name: &str) -> Result<PathBuf> {
    let cache_dir = get_model_cache_dir()?;
    
    // Find the model info
    let model_info = AVAILABLE_MODELS
        .iter()
        .find(|(name, _, _, _)| *name == model_name)
        .ok_or_else(|| ModelError::NotFound(format!("Model '{}' not found", model_name)))?;
    
    Ok(cache_dir.join(model_info.2))
}

/// Check if a model is downloaded
pub fn is_model_downloaded(model_name: &str) -> bool {
    get_model_path(model_name)
        .map(|path| path.exists())
        .unwrap_or(false)
}

/// List all available models with their status
pub fn list_models() -> Vec<LlmModelInfo> {
    AVAILABLE_MODELS
        .iter()
        .map(|(name, repo, filename, size_mb)| {
            let downloaded = is_model_downloaded(name);
            LlmModelInfo {
                name: name.to_string(),
                size_mb: *size_mb,
                downloaded,
                description: format!("SmolLM2 {} quantized model", name.split('-').nth(1).unwrap_or("")),
                hf_repo: repo.to_string(),
                filename: filename.to_string(),
            }
        })
        .collect()
}

/// Download a model from HuggingFace
pub async fn download_model(
    model_name: &str,
    progress_callback: Option<impl Fn(u64, u64) + Send + Sync>,
) -> Result<PathBuf> {
    let model_info = AVAILABLE_MODELS
        .iter()
        .find(|(name, _, _, _)| *name == model_name)
        .ok_or_else(|| ModelError::NotFound(format!("Model '{}' not found", model_name)))?;

    let cache_dir = get_model_cache_dir()?;
    let model_path = cache_dir.join(model_info.2);

    // Skip if already downloaded
    if model_path.exists() {
        tracing::info!("Model {} already exists at {:?}", model_name, model_path);
        return Ok(model_path);
    }

    // Construct HuggingFace URL
    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}",
        model_info.1, model_info.2
    );

    tracing::info!("Downloading {} from {}", model_name, url);

    // Download with progress tracking
    let response = ureq::get(&url)
        .call()
        .map_err(|e| ModelError::DownloadFailed(e.to_string()))?;

    let total_size = response
        .header("content-length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&model_path)?;
    let mut downloaded = 0u64;
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = std::io::Read::read(&mut reader, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buffer[..bytes_read])?;
        downloaded += bytes_read as u64;

        if let Some(ref callback) = progress_callback {
            callback(downloaded, total_size);
        }
    }

    tracing::info!("Model {} downloaded to {:?}", model_name, model_path);
    Ok(model_path)
}
