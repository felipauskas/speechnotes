use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiagnosticEvent {
    pub session_id: String,
    pub engine_id: String,
    pub model_id: String,
    pub audio_duration_ms: u64,
    pub processing_time_ms: u64,
    pub is_silent: bool,
    pub status: String,
    pub error_code: Option<String>,
}

pub struct DiagnosticLogger;

impl DiagnosticLogger {
    pub fn log_event(event: DiagnosticEvent) {
        info!(
            target: "stt_diagnostics",
            session_id = %event.session_id,
            engine = %event.engine_id,
            model = %event.model_id,
            audio_duration_ms = event.audio_duration_ms,
            processing_time_ms = event.processing_time_ms,
            is_silent = event.is_silent,
            status = %event.status,
            error_code = ?event.error_code,
            "STT Execution Completed"
        );
    }
}
