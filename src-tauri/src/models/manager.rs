use crate::errors::{AppError, AppErrorCode, AppResult};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::info;
use ts_rs::TS;

const MODEL_ID: &str = "whisper-large-v3-mlx";
const MODEL_DIR: &str = "mlx-whisper-large-v3";
pub const MODEL_REVISION: &str = "49e6aa286ad60c14352c404340ded53710378a11";
const MODEL_SIZE: u64 = 3_083_520_416;
pub const MODEL_SHA256: &str = "05ff791ce3630fae47e7c51004e9666204d786246ec07cac6110af768099b40d";
const CONFIG_SHA256: &str = "34982ce6ae286095000f82ae9583b3431639e8b092bf60c961f203745e6500e3";
const MODEL_BASE_URL: &str = "https://huggingface.co/mlx-community/whisper-large-v3-mlx/resolve/49e6aa286ad60c14352c404340ded53710378a11";

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub url: String,
    pub expected_sha256: String,
    pub expected_size_bytes: u64,
    pub is_installed: bool,
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct DownloadProgressPayload {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress: f64,
}

pub struct ModelManager {
    models_dir: PathBuf,
    http_client: Client,
    download_lock: Mutex<()>,
}

struct StagingDirectory(PathBuf);

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&models_dir).ok();
        Self {
            models_dir,
            http_client: Client::builder().build().unwrap_or_default(),
            download_lock: Mutex::new(()),
        }
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        vec![self.model_info()]
    }

    pub fn is_model_ready(&self) -> bool {
        self.get_mlx_model_path().is_ok()
    }

    pub fn get_mlx_model_path(&self) -> AppResult<PathBuf> {
        let path = self.models_dir.join(MODEL_DIR);
        let config_path = path.join("config.json");
        let weights_path = path.join("weights.npz");
        let weights_size = weights_path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        if !config_path.is_file() || !weights_path.is_file() || weights_size != MODEL_SIZE {
            return Err(AppError::new(
                AppErrorCode::ModelNotFound,
                "MLX Whisper Large v3 is not installed or is incomplete",
            ));
        }

        Ok(path)
    }

    pub async fn download_model<F>(&self, model_id: &str, progress_cb: F) -> AppResult<PathBuf>
    where
        F: Fn(DownloadProgressPayload) + Send + Sync + 'static,
    {
        if model_id != MODEL_ID {
            return Err(AppError::new(
                AppErrorCode::ModelNotFound,
                format!("Unknown model: {model_id}"),
            ));
        }

        let _download_guard = self.download_lock.lock().await;
        if let Ok(path) = self.get_mlx_model_path() {
            return Ok(path);
        }

        let staging_path = self.models_dir.join(format!("{MODEL_DIR}.part"));
        let final_path = self.models_dir.join(MODEL_DIR);
        if staging_path.exists() {
            std::fs::remove_dir_all(&staging_path).map_err(|error| {
                AppError::new(
                    AppErrorCode::InternalError,
                    format!("Failed to clear incomplete model: {error}"),
                )
            })?;
        }
        std::fs::create_dir_all(&staging_path).map_err(|error| {
            AppError::new(
                AppErrorCode::InternalError,
                format!("Failed to create model staging directory: {error}"),
            )
        })?;
        let _staging_cleanup = StagingDirectory(staging_path.clone());

        let files = [
            (format!("{MODEL_BASE_URL}/config.json"), "config.json"),
            (format!("{MODEL_BASE_URL}/weights.npz"), "weights.npz"),
        ];
        let total_bytes = MODEL_SIZE + 269;
        let mut downloaded_bytes = 0_u64;

        for (url, filename) in files {
            info!("Downloading {filename} for {MODEL_ID}");
            let response = self.http_client.get(&url).send().await.map_err(|error| {
                AppError::new(
                    AppErrorCode::ModelIncompatible,
                    format!("Model download request failed: {error}"),
                )
            })?;
            if !response.status().is_success() {
                return Err(AppError::new(
                    AppErrorCode::ModelIncompatible,
                    format!("Model download returned HTTP {}", response.status()),
                ));
            }

            let file = File::create(staging_path.join(filename)).map_err(|error| {
                AppError::new(
                    AppErrorCode::InternalError,
                    format!("Failed to create model file: {error}"),
                )
            })?;
            let mut writer = BufWriter::new(file);
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| {
                    AppError::new(
                        AppErrorCode::ModelIncompatible,
                        format!("Model download failed: {error}"),
                    )
                })?;
                writer.write_all(&chunk).map_err(|error| {
                    AppError::new(
                        AppErrorCode::InternalError,
                        format!("Failed writing model file: {error}"),
                    )
                })?;
                downloaded_bytes += chunk.len() as u64;
                progress_cb(DownloadProgressPayload {
                    model_id: MODEL_ID.to_string(),
                    downloaded_bytes,
                    total_bytes,
                    progress: downloaded_bytes as f64 / total_bytes as f64,
                });
            }
            writer.flush().map_err(|error| {
                AppError::new(
                    AppErrorCode::InternalError,
                    format!("Failed finalizing model file: {error}"),
                )
            })?;
        }

        self.verify_sha256(&staging_path.join("config.json"), CONFIG_SHA256)?;
        self.verify_sha256(&staging_path.join("weights.npz"), MODEL_SHA256)?;
        if final_path.exists() {
            std::fs::remove_dir_all(&final_path).map_err(|error| {
                AppError::new(
                    AppErrorCode::InternalError,
                    format!("Failed replacing model directory: {error}"),
                )
            })?;
        }
        std::fs::rename(&staging_path, &final_path).map_err(|error| {
            AppError::new(
                AppErrorCode::InternalError,
                format!("Failed activating model: {error}"),
            )
        })?;
        Ok(final_path)
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            id: MODEL_ID.to_string(),
            name: "Whisper Large v3 (MLX, 3.1 GB)".to_string(),
            filename: MODEL_DIR.to_string(),
            url: format!("{MODEL_BASE_URL}/weights.npz"),
            expected_sha256: MODEL_SHA256.to_string(),
            expected_size_bytes: MODEL_SIZE,
            is_installed: self.is_model_ready(),
            is_default: true,
        }
    }

    fn verify_sha256(&self, path: &Path, expected_sha256: &str) -> AppResult<()> {
        let mut file = File::open(path).map_err(|error| {
            AppError::new(
                AppErrorCode::ModelNotFound,
                format!("Failed to open model for verification: {error}"),
            )
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 1024 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|error| {
                AppError::new(
                    AppErrorCode::ModelChecksumMismatch,
                    format!("Failed reading model for verification: {error}"),
                )
            })?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }

        let actual = format!("{:x}", hasher.finalize());
        if actual.eq_ignore_ascii_case(expected_sha256) {
            Ok(())
        } else {
            Err(AppError::new(
                AppErrorCode::ModelChecksumMismatch,
                format!("Model SHA-256 mismatch: got {actual}"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_model_is_not_ready() {
        let models_dir = std::env::temp_dir().join(format!(
            "speechnotes-model-readiness-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let model_dir = models_dir.join(MODEL_DIR);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("config.json"), b"{}").unwrap();
        std::fs::write(model_dir.join("weights.npz"), b"incomplete").unwrap();

        let manager = ModelManager::new(models_dir.clone());
        assert!(!manager.is_model_ready());
        assert!(!manager.list_models()[0].is_installed);

        std::fs::remove_dir_all(models_dir).unwrap();
    }
}
