//! Model loading and downloading utilities.
//!
//! Supports loading ONNX models from:
//! - Local path (user-provided)
//! - Embedded/bundled models
//! - Downloaded models (cached locally)

use crate::error::{CompressError, Result};
use std::path::{Path, PathBuf};

/// Default model directory name.
pub const DEFAULT_MODEL_DIR: &str = "models";

/// Environment variable for model path override.
pub const MODEL_PATH_ENV: &str = "PROMPT_COMPRESS_MODEL";

/// Get the model directory path.
///
/// Resolution order:
/// 1. `PROMPT_COMPRESS_MODEL` environment variable
/// 2. Default models/ directory in project root
/// 3. Current working directory + models/
pub fn get_model_dir() -> PathBuf {
    // Check environment variable first
    if let Ok(path) = std::env::var(MODEL_PATH_ENV) {
        return PathBuf::from(path);
    }

    // Try to find models directory relative to executable or current dir
    if let Ok(cwd) = std::env::current_dir() {
        let models_dir = cwd.join(DEFAULT_MODEL_DIR);
        if models_dir.exists() {
            return models_dir;
        }
    }

    // Fallback to current directory
    PathBuf::from(DEFAULT_MODEL_DIR)
}

/// Check if a model exists at the given directory.
///
/// A valid model directory must contain:
/// - `model.onnx` - The ONNX model file
/// - `tokenizer.json` - HuggingFace tokenizer config
pub fn model_exists(model_dir: &Path) -> bool {
    model_dir.join("model.onnx").exists() && model_dir.join("tokenizer.json").exists()
}

/// Find the best available model directory.
///
/// Returns the first directory that contains a valid model,
/// or an error if no model is found.
pub fn find_model() -> Result<PathBuf> {
    // Try the configured model directory
    let model_dir = get_model_dir();
    if model_exists(&model_dir) {
        return Ok(model_dir);
    }

    // Try common alternative locations
    let alternatives = [
        PathBuf::from("models/scorer-v0.1"),
        PathBuf::from("../models"),
        PathBuf::from("../../models"),
    ];

    for alt in &alternatives {
        if model_exists(alt) {
            return Ok(alt.clone());
        }
    }

    Err(CompressError::Model(format!(
        "No model found. Expected model.onnx and tokenizer.json in {} or standard locations. \
         Set {} environment variable to specify the model directory.",
        model_dir.display(),
        MODEL_PATH_ENV
    )))
}

/// Download model from a URL to the cache directory.
#[cfg(feature = "download")]
pub async fn download_model(base_url: &str, version: &str) -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("prompt-compress")
        .join(version);

    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| CompressError::Model(format!("failed to create cache dir: {e}")))?;

    let model_path = cache_dir.join("model.onnx");
    let tokenizer_path = cache_dir.join("tokenizer.json");

    // Download if not cached
    if !model_path.exists() {
        let model_url = format!("{}/{}/model.onnx", base_url, version);
        download_file(&model_url, &model_path).await?;
    }

    if !tokenizer_path.exists() {
        let tokenizer_url = format!("{}/{}/tokenizer.json", base_url, version);
        download_file(&tokenizer_url, &tokenizer_path).await?;
    }

    Ok(cache_dir)
}

#[cfg(feature = "download")]
async fn download_file(url: &str, dest: &Path) -> Result<()> {
    use reqwest;
    use std::io::Write;

    let response = reqwest::get(url)
        .await
        .map_err(|e| CompressError::Model(format!("download failed: {e}")))?;

    if !response.status().is_success() {
        return Err(CompressError::Model(format!(
            "download failed with status: {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| CompressError::Model(format!("failed to read response: {e}")))?;

    let mut file = std::fs::File::create(dest)
        .map_err(|e| CompressError::Model(format!("failed to create file: {e}")))?;

    file.write_all(&bytes)
        .map_err(|e| CompressError::Model(format!("failed to write file: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_model_dir_env() {
        // Set env var and test
        std::env::set_var(MODEL_PATH_ENV, "/custom/model/path");
        let dir = get_model_dir();
        assert_eq!(dir, PathBuf::from("/custom/model/path"));
        std::env::remove_var(MODEL_PATH_ENV);
    }

    #[test]
    fn test_get_model_dir_default() {
        // Without env var, should return default
        std::env::remove_var(MODEL_PATH_ENV);
        let dir = get_model_dir();
        assert!(dir.to_string_lossy().contains(DEFAULT_MODEL_DIR));
    }
}
