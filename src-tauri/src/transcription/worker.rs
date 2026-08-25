use crate::errors::{AppError, AppErrorCode, AppResult};
use crate::transcription::{WorkerRequest, WorkerResponse, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::info;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const MODEL_PREPARE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionResultPayload {
    pub text: String,
    pub detected_language: Option<String>,
    pub duration_ms: u64,
    pub processing_time_ms: u64,
}

pub struct WorkerSupervisor {
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout_reader: Arc<Mutex<Option<BufReader<ChildStdout>>>>,
    prepared_model: Arc<Mutex<Option<String>>>,
    operation_lock: Arc<Mutex<()>>,
    worker_binary_path: PathBuf,
}

impl WorkerSupervisor {
    pub fn new(worker_binary_path: PathBuf) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            stdin: Arc::new(Mutex::new(None)),
            stdout_reader: Arc::new(Mutex::new(None)),
            prepared_model: Arc::new(Mutex::new(None)),
            operation_lock: Arc::new(Mutex::new(())),
            worker_binary_path,
        }
    }

    async fn ensure_started(&self) -> AppResult<()> {
        let mut child_lock = self.child.lock().await;
        if let Some(child) = child_lock.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) | Err(_) => {
                    child_lock.take();
                    *self.stdin.lock().await = None;
                    *self.stdout_reader.lock().await = None;
                    *self.prepared_model.lock().await = None;
                }
            }
        }

        info!("Spawning STT worker: {:?}", self.worker_binary_path);
        let mut child = Command::new(&self.worker_binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                AppError::new(
                    AppErrorCode::WorkerCrashed,
                    format!("Failed to spawn worker: {error}"),
                )
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            AppError::new(
                AppErrorCode::WorkerCrashed,
                "Failed to capture worker stdin",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AppError::new(
                AppErrorCode::WorkerCrashed,
                "Failed to capture worker stdout",
            )
        })?;

        *self.stdin.lock().await = Some(stdin);
        *self.stdout_reader.lock().await = Some(BufReader::new(stdout));
        *child_lock = Some(child);
        drop(child_lock);

        let request = WorkerRequest::Ping {
            id: "worker-handshake".to_string(),
        };
        match self.send_and_read(&request, HANDSHAKE_TIMEOUT).await? {
            WorkerResponse::Pong {
                protocol_version, ..
            } if protocol_version == PROTOCOL_VERSION => Ok(()),
            WorkerResponse::Pong {
                protocol_version, ..
            } => {
                self.reset().await;
                Err(AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!(
                        "Worker protocol {protocol_version} is incompatible with required version {PROTOCOL_VERSION}"
                    ),
                ))
            }
            response => {
                self.reset().await;
                Err(AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!("Unexpected handshake response: {response:?}"),
                ))
            }
        }
    }

    pub async fn transcribe(
        &self,
        id: &str,
        audio_path: &str,
        model_path: &str,
        model_id: &str,
        audio_duration_ms: Option<u64>,
        initial_prompt: Option<String>,
    ) -> AppResult<TranscriptionResultPayload> {
        let _operation_guard = self.operation_lock.lock().await;
        self.ensure_started().await?;
        self.prepare_model(model_path, model_id).await?;

        // Dynamic watchdog: allow at least 180s, scaling with audio duration up to hours
        let timeout_ms = match audio_duration_ms {
            Some(dur) => std::cmp::max(180_000, dur.saturating_mul(2)),
            None => 1_800_000, // 30 minutes default
        };

        let request = WorkerRequest::Transcribe {
            id: id.to_string(),
            audio_path: audio_path.to_string(),
            language: "en".to_string(),
            initial_prompt,
        };

        match self
            .send_and_read(&request, Duration::from_millis(timeout_ms))
            .await?
        {
            WorkerResponse::Result {
                text,
                detected_language,
                audio_duration_ms,
                processing_time_ms,
                ..
            } => Ok(TranscriptionResultPayload {
                text,
                detected_language,
                duration_ms: audio_duration_ms,
                processing_time_ms,
            }),
            WorkerResponse::Error { code, message, .. } => Err(AppError::new(
                AppErrorCode::WorkerProtocolError,
                format!("{code}: {message}"),
            )),
            response => {
                self.reset().await;
                Err(AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!("Unexpected transcription response: {response:?}"),
                ))
            }
        }
    }

    pub async fn warm_up(&self, model_path: &str, model_id: &str) -> AppResult<()> {
        let _operation_guard = self.operation_lock.lock().await;
        self.ensure_started().await?;
        self.prepare_model(model_path, model_id).await
    }

    async fn prepare_model(&self, model_path: &str, model_id: &str) -> AppResult<()> {
        if self.prepared_model.lock().await.as_deref() == Some(model_path) {
            return Ok(());
        }

        let request = WorkerRequest::PrepareModel {
            id: "prepare-model".to_string(),
            model_dir: model_path.to_string(),
            model_id: model_id.to_string(),
        };

        match self.send_and_read(&request, MODEL_PREPARE_TIMEOUT).await? {
            WorkerResponse::ModelReady { .. } => {
                *self.prepared_model.lock().await = Some(model_path.to_string());
                Ok(())
            }
            WorkerResponse::Error { code, message, .. } => Err(AppError::new(
                AppErrorCode::ModelIncompatible,
                format!("{code}: {message}"),
            )),
            response => {
                self.reset().await;
                Err(AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!("Unexpected model response: {response:?}"),
                ))
            }
        }
    }

    async fn send_and_read(
        &self,
        request: &WorkerRequest,
        request_timeout: Duration,
    ) -> AppResult<WorkerResponse> {
        match timeout(request_timeout, self.send_and_read_inner(request)).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => {
                self.reset().await;
                Err(error)
            }
            Err(_) => {
                self.reset().await;
                Err(AppError::new(
                    AppErrorCode::WorkerTimeout,
                    format!("Worker request {} timed out", request.id()),
                ))
            }
        }
    }

    async fn send_and_read_inner(&self, request: &WorkerRequest) -> AppResult<WorkerResponse> {
        let mut encoded = serde_json::to_vec(request).map_err(|error| {
            AppError::new(
                AppErrorCode::WorkerProtocolError,
                format!("Failed to encode worker request: {error}"),
            )
        })?;
        encoded.push(b'\n');

        {
            let mut stdin_lock = self.stdin.lock().await;
            let stdin = stdin_lock.as_mut().ok_or_else(|| {
                AppError::new(AppErrorCode::WorkerCrashed, "Worker stdin unavailable")
            })?;
            stdin.write_all(&encoded).await.map_err(|error| {
                AppError::new(
                    AppErrorCode::WorkerCrashed,
                    format!("Failed to write worker request: {error}"),
                )
            })?;
            stdin.flush().await.map_err(|error| {
                AppError::new(
                    AppErrorCode::WorkerCrashed,
                    format!("Failed to flush worker request: {error}"),
                )
            })?;
        }

        let mut reader_lock = self.stdout_reader.lock().await;
        let reader = reader_lock.as_mut().ok_or_else(|| {
            AppError::new(AppErrorCode::WorkerCrashed, "Worker stdout unavailable")
        })?;
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await.map_err(|error| {
                AppError::new(
                    AppErrorCode::WorkerCrashed,
                    format!("Failed reading worker response: {error}"),
                )
            })?;
            if bytes_read == 0 {
                return Err(AppError::new(
                    AppErrorCode::WorkerCrashed,
                    "Worker exited before producing a terminal response",
                ));
            }

            let response: WorkerResponse = serde_json::from_str(line.trim()).map_err(|error| {
                AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!("Invalid worker response: {error}"),
                )
            })?;
            if response.id() != request.id() {
                return Err(AppError::new(
                    AppErrorCode::WorkerProtocolError,
                    format!(
                        "Worker response ID {} does not match request ID {}",
                        response.id(),
                        request.id()
                    ),
                ));
            }
            if !matches!(response, WorkerResponse::Progress { .. }) {
                return Ok(response);
            }
        }
    }

    pub async fn reset(&self) {
        let mut child_lock = self.child.lock().await;
        if let Some(mut child) = child_lock.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        *self.stdin.lock().await = None;
        *self.stdout_reader.lock().await = None;
        *self.prepared_model.lock().await = None;
    }

    pub async fn shutdown(&self) {
        let _operation_guard = self.operation_lock.lock().await;
        self.reset().await;
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires SPEECH_NOTES_MLX_WORKER, SPEECH_NOTES_MLX_MODEL, and SPEECH_NOTES_TEST_AUDIO"]
    async fn mlx_worker_transcribes_reference_audio() {
        let worker_path = std::env::var("SPEECH_NOTES_MLX_WORKER").unwrap();
        let model_path = std::env::var("SPEECH_NOTES_MLX_MODEL").unwrap();
        let audio_path = std::env::var("SPEECH_NOTES_TEST_AUDIO").unwrap();
        let supervisor = WorkerSupervisor::new(worker_path.into());

        let result = supervisor
            .transcribe(
                "reference-audio",
                &audio_path,
                &model_path,
                "whisper-large-v3-mlx",
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            result.text,
            "So right now we are doing this audio as a, how can you call it, like a test. Like my own voice speaking because we want to cross-check and confirm and validate that our speech-to-text is in its optimal state."
        );
        assert_eq!(result.duration_ms, 32_213);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "requires SPEECH_NOTES_MLX_WORKER, SPEECH_NOTES_MLX_MODEL, and SPEECH_NOTES_SILENCE_AUDIO"]
    async fn mlx_worker_suppresses_no_speech_hallucination() {
        let worker_path = std::env::var("SPEECH_NOTES_MLX_WORKER").unwrap();
        let model_path = std::env::var("SPEECH_NOTES_MLX_MODEL").unwrap();
        let audio_path = std::env::var("SPEECH_NOTES_SILENCE_AUDIO").unwrap();
        let supervisor = WorkerSupervisor::new(worker_path.into());

        let result = supervisor
            .transcribe(
                "no-speech-audio",
                &audio_path,
                &model_path,
                "whisper-large-v3-mlx",
                None,
                None,
            )
            .await
            .unwrap();

        assert!(result.text.is_empty());
        supervisor.shutdown().await;
    }
}
