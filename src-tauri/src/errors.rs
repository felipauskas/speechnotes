use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub enum AppErrorCode {
    MicrophonePermissionDenied,
    NoInputDevice,
    AudioCaptureFailed,
    NoSpeechDetected,
    SessionAlreadyActive,
    SessionNotFound,
    WorkerCrashed,
    WorkerTimeout,
    WorkerProtocolError,
    ModelNotFound,
    ModelChecksumMismatch,
    ModelIncompatible,
    DatabaseError,
    ClipboardFailed,
    InternalError,
}

impl fmt::Display for AppErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

pub type AppResult<T> = Result<T, AppError>;
