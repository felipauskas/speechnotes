use crate::errors::{AppError, AppErrorCode, AppResult};
use crate::persistence::TranscriptionRepository;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct SessionStatePayload {
    pub boot_id: String,
    pub session_id: Option<String>,
    pub state: SessionState,
    pub revision: u64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

pub struct SessionManager {
    boot_id: String,
    revision: AtomicU64,
    current_state: Mutex<SessionState>,
    active_session_id: Mutex<Option<String>>,
    repository: Arc<TranscriptionRepository>,
}

impl SessionManager {
    pub fn new(boot_id: String, repository: Arc<TranscriptionRepository>) -> Self {
        Self {
            boot_id,
            revision: AtomicU64::new(1),
            current_state: Mutex::new(SessionState::Idle),
            active_session_id: Mutex::new(None),
            repository,
        }
    }

    pub fn next_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn current_state(&self) -> SessionState {
        let lock = self.current_state.lock().await;
        lock.clone()
    }

    pub async fn active_session_id(&self) -> Option<String> {
        let lock = self.active_session_id.lock().await;
        lock.clone()
    }

    pub async fn start_session(&self, session_id: String) -> AppResult<SessionStatePayload> {
        let mut state_lock = self.current_state.lock().await;
        if *state_lock != SessionState::Idle {
            return Err(AppError::new(
                AppErrorCode::SessionAlreadyActive,
                "A session is already active",
            ));
        }

        // Persist the session before microphone activation; audio is never persisted.
        if let Err(error) = self.repository.create_session(&session_id) {
            *state_lock = SessionState::Idle;
            return Err(error);
        }

        let mut id_lock = self.active_session_id.lock().await;
        *id_lock = Some(session_id.clone());
        *state_lock = SessionState::Recording;
        info!("Session {} started (state: Recording)", session_id);

        let rev = self.next_revision();
        Ok(SessionStatePayload {
            boot_id: self.boot_id.clone(),
            session_id: Some(session_id),
            state: SessionState::Recording,
            revision: rev,
            error_code: None,
            error_message: None,
        })
    }

    pub async fn transition_to(&self, new_state: SessionState) -> AppResult<SessionStatePayload> {
        let mut state_lock = self.current_state.lock().await;
        let session_id = self.active_session_id.lock().await.clone();

        *state_lock = new_state.clone();
        let rev = self.next_revision();

        Ok(SessionStatePayload {
            boot_id: self.boot_id.clone(),
            session_id,
            state: new_state,
            revision: rev,
            error_code: None,
            error_message: None,
        })
    }

    pub async fn complete_session(&self) -> AppResult<SessionStatePayload> {
        let mut state_lock = self.current_state.lock().await;
        let mut id_lock = self.active_session_id.lock().await;

        let session_id = id_lock.take();
        *state_lock = SessionState::Idle;
        let rev = self.next_revision();

        Ok(SessionStatePayload {
            boot_id: self.boot_id.clone(),
            session_id,
            state: SessionState::Idle,
            revision: rev,
            error_code: None,
            error_message: None,
        })
    }

    pub async fn fail_session(
        &self,
        code: AppErrorCode,
        message: &str,
    ) -> AppResult<SessionStatePayload> {
        let mut state_lock = self.current_state.lock().await;
        let mut id_lock = self.active_session_id.lock().await;

        let session_id = id_lock.clone();

        if let Some(ref id) = session_id {
            let _ = self
                .repository
                .fail_transcription(id, &format!("{:?}", code), message);
        }

        *state_lock = SessionState::Idle;
        *id_lock = None;
        let rev = self.next_revision();

        Ok(SessionStatePayload {
            boot_id: self.boot_id.clone(),
            session_id,
            state: SessionState::Idle,
            revision: rev,
            error_code: Some(format!("{:?}", code)),
            error_message: Some(message.to_string()),
        })
    }
}
