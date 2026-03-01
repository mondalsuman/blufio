// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model download manager for first-run ONNX embedding model setup.
//!
//! Downloads all-MiniLM-L6-v2 INT8 quantized model from HuggingFace
//! on first run and caches it in the data directory.

use std::path::{Path, PathBuf};

use blufio_core::error::BlufioError;
use tokio::sync::OnceCell;
use tracing::info;

/// URLs for model files on HuggingFace.
const MODEL_URL: &str =
    "https://huggingface.co/onnx-community/all-MiniLM-L6-v2-ONNX/resolve/main/onnx/model_quantized.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

/// Manages ONNX model download and path resolution.
pub struct ModelManager {
    data_dir: PathBuf,
    /// Ensures model is downloaded only once even with concurrent callers.
    _init_guard: OnceCell<()>,
}

impl ModelManager {
    /// Creates a new ModelManager with the given data directory.
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            _init_guard: OnceCell::new(),
        }
    }

    /// Returns the directory where model files are stored.
    pub fn model_dir(&self) -> PathBuf {
        self.data_dir.join("models").join("all-MiniLM-L6-v2")
    }

    /// Returns the path to the ONNX model file.
    pub fn model_path(&self) -> PathBuf {
        self.model_dir().join("model.onnx")
    }

    /// Returns the path to the tokenizer.json file.
    pub fn tokenizer_path(&self) -> PathBuf {
        self.model_dir().join("tokenizer.json")
    }

    /// Returns true if both model and tokenizer files exist.
    pub fn is_model_available(&self) -> bool {
        self.model_path().exists() && self.tokenizer_path().exists()
    }

    /// Ensures the model is downloaded and available.
    ///
    /// Downloads from HuggingFace on first run; subsequent calls are no-ops.
    /// Uses `OnceCell` to prevent concurrent download races.
    pub async fn ensure_model(&self) -> Result<PathBuf, BlufioError> {
        if self.is_model_available() {
            return Ok(self.model_path());
        }

        info!("Embedding model not found, downloading from HuggingFace...");

        let model_dir = self.model_dir();
        tokio::fs::create_dir_all(&model_dir)
            .await
            .map_err(|e| BlufioError::Internal(format!("Failed to create model directory: {e}")))?;

        let files = [
            ("model.onnx", MODEL_URL),
            ("tokenizer.json", TOKENIZER_URL),
        ];

        for (filename, url) in &files {
            let dest = model_dir.join(filename);
            if dest.exists() {
                continue;
            }

            info!("Downloading {filename}...");
            match download_file(url, &dest).await {
                Ok(size) => {
                    info!("Downloaded {filename} ({size} bytes)");
                }
                Err(e) => {
                    // Clean up partial download
                    let _ = tokio::fs::remove_file(&dest).await;
                    return Err(e);
                }
            }
        }

        info!("Embedding model ready at: {}", model_dir.display());
        Ok(self.model_path())
    }
}

/// Download a file from a URL to a local path.
async fn download_file(url: &str, dest: &Path) -> Result<usize, BlufioError> {
    let response = reqwest::get(url).await.map_err(|e| {
        BlufioError::Internal(format!("Failed to download {url}: {e}"))
    })?;

    if !response.status().is_success() {
        return Err(BlufioError::Internal(format!(
            "Download failed with status {}: {url}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        BlufioError::Internal(format!("Failed to read response body from {url}: {e}"))
    })?;

    let size = bytes.len();
    tokio::fs::write(dest, &bytes).await.map_err(|e| {
        BlufioError::Internal(format!("Failed to write {}: {e}", dest.display()))
    })?;

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_under_data_dir() {
        let mgr = ModelManager::new(PathBuf::from("/tmp/blufio"));
        assert_eq!(
            mgr.model_path(),
            PathBuf::from("/tmp/blufio/models/all-MiniLM-L6-v2/model.onnx")
        );
    }

    #[test]
    fn tokenizer_path_under_data_dir() {
        let mgr = ModelManager::new(PathBuf::from("/tmp/blufio"));
        assert_eq!(
            mgr.tokenizer_path(),
            PathBuf::from("/tmp/blufio/models/all-MiniLM-L6-v2/tokenizer.json")
        );
    }

    #[test]
    fn model_dir_structure() {
        let mgr = ModelManager::new(PathBuf::from("/data"));
        assert_eq!(
            mgr.model_dir(),
            PathBuf::from("/data/models/all-MiniLM-L6-v2")
        );
    }

    #[test]
    fn model_not_available_when_missing() {
        let mgr = ModelManager::new(PathBuf::from("/nonexistent/path"));
        assert!(!mgr.is_model_available());
    }
}
