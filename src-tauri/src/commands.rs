use crate::audio::{AudioDeviceInfo, AudioPreprocessor, DeviceManager};
use crate::clipboard::ClipboardManager;
use crate::errors::{AppError, AppErrorCode, AppResult};
use crate::models::{ModelInfo, ModelManager, MODEL_REVISION, MODEL_SHA256};
use crate::permissions::{PermissionManager, PermissionStatus};
use crate::persistence::{TranscriptionCompletion, TranscriptionRecord, TranscriptionRepository};
use crate::session::{SessionManager, SessionStatePayload};
use crate::transcription::WorkerSupervisor;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct AppState {
    pub boot_id: String,
    pub session_manager: Arc<SessionManager>,
    pub repository: Arc<TranscriptionRepository>,
    pub model_manager: Arc<ModelManager>,
    pub worker_supervisor: Arc<WorkerSupervisor>,
    pub audio_recorder: Arc<Mutex<crate::audio::AudioRecorder>>,
    pub record_path_buf: Arc<Mutex<Option<PathBuf>>>,
    pub record_session_id: Arc<Mutex<Option<String>>>,
    pub tray: tauri::tray::TrayIcon,
}

#[tauri::command]
pub async fn get_application_state(state: State<'_, AppState>) -> AppResult<SessionStatePayload> {
    let session_id = state.session_manager.active_session_id().await;
    let current_state = state.session_manager.current_state().await;
    let rev = state.session_manager.next_revision();

    Ok(SessionStatePayload {
        boot_id: state.boot_id.clone(),
        session_id,
        state: current_state,
        revision: rev,
        error_code: None,
        error_message: None,
    })
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<SessionStatePayload> {
    if !state.model_manager.is_model_ready() {
        hide_overlay(app.clone())?;
        show_settings_window(&app)?;
        return Err(AppError::new(
            AppErrorCode::ModelNotFound,
            "Install the 3.1 GB English transcription model before recording",
        ));
    }

    show_overlay(app.clone())?;

    let microphone_permission = match PermissionManager::check_microphone_permission() {
        PermissionStatus::NotDetermined => PermissionManager::request_microphone_permission().await,
        status => status,
    };

    if microphone_permission != PermissionStatus::Authorized {
        let error = AppError::new(
            AppErrorCode::MicrophonePermissionDenied,
            "Microphone access is not authorized",
        );
        emit_idle_error(&app, &state, &error).await;
        return Err(error);
    }

    let session_id = Uuid::new_v4().to_string();
    let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
        app.path()
            .home_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".speechnotes")
    });
    let audio_dir = app_data_dir.join("recordings");
    std::fs::create_dir_all(&audio_dir).ok();

    let wav_path = audio_dir.join(format!("{}.wav", session_id));
    let payload = state
        .session_manager
        .start_session(session_id.clone())
        .await?;

    // Store session identifiers BEFORE starting hardware capture to avoid race conditions
    *state.record_path_buf.lock().await = Some(wav_path.clone());
    *state.record_session_id.lock().await = Some(session_id.clone());

    let recording_result = async {
        let preferred_device = state.repository.get_setting("audio.inputDevice")?;
        let mut recorder = state.audio_recorder.lock().await;
        recorder.start_recording(wav_path.clone(), preferred_device.as_deref())
    }
    .await;

    if let Err(error) = recording_result {
        let _ = remove_runtime_audio(&wav_path);
        *state.record_path_buf.lock().await = None;
        *state.record_session_id.lock().await = None;
        if let Ok(failure_payload) = state
            .session_manager
            .fail_session(error.code.clone(), &error.message)
            .await
        {
            let _ = app.emit("session-state-changed", &failure_payload);
        }
        return Err(error);
    }

    // Spawn 28Hz meter telemetry publisher loop for live waveform UI
    let app_meter_handle = app.clone();
    let recorder_meter_ref = state.audio_recorder.clone();
    let meter_session_id = session_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(35));
        loop {
            interval.tick().await;
            let (is_rec, level) = {
                let recorder = recorder_meter_ref.lock().await;
                (recorder.is_recording(), recorder.current_level())
            };
            if !is_rec {
                break;
            }
            let payload = serde_json::json!({
                "session_id": meter_session_id,
                "level": level,
            });
            let _ = app_meter_handle.emit("audio-level-changed", payload);
        }
    });

    let _ = app.emit("session-state-changed", &payload);
    Ok(payload)
}

#[tauri::command]
pub async fn toggle_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<SessionStatePayload> {
    let current_state = state.session_manager.current_state().await;
    match current_state {
        crate::session::SessionState::Idle => start_recording(app, state).await,
        crate::session::SessionState::Recording => {
            let record_res = stop_recording(app, state.clone()).await;
            match record_res {
                Ok(_) => get_application_state(state).await,
                Err(e) => Err(e),
            }
        }
        _ => get_application_state(state).await,
    }
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<TranscriptionRecord> {
    let session_id = state.record_session_id.lock().await.take();
    let wav_path = state.record_path_buf.lock().await.take();
    let (session_id, wav_path) = match (session_id, wav_path) {
        (Some(session_id), Some(wav_path)) => (session_id, wav_path),
        (_, wav_path) => {
            let _ = state.audio_recorder.lock().await.stop_recording();
            if let Some(wav_path) = wav_path {
                let _ = remove_runtime_audio(&wav_path);
            }
            return Err(AppError::new(
                AppErrorCode::SessionNotFound,
                "Active recording state is inconsistent",
            ));
        }
    };

    let process_transcription = async {
        let _samples_written = state.audio_recorder.lock().await.stop_recording()?;

        let transcribing_payload = state
            .session_manager
            .transition_to(crate::session::SessionState::Transcribing)
            .await;
        if let Ok(ref payload) = transcribing_payload {
            let _ = app.emit("session-state-changed", payload);
        }

        let prep_info = AudioPreprocessor::prepare_and_validate(&wav_path)?;
        let audio_duration_ms = prep_info.duration_ms as i64;

        let model_path = state.model_manager.get_mlx_model_path()?;
        let selected_engine = "mlx-whisper".to_string();
        let selected_model_id = "whisper-large-v3-mlx".to_string();

        let custom_vocabulary = state
            .repository
            .get_setting("transcription.customVocabulary")?
            .filter(|v| !v.trim().is_empty());

        let res = state
            .worker_supervisor
            .transcribe(
                &session_id,
                &wav_path.to_string_lossy(),
                &model_path.to_string_lossy(),
                &selected_model_id,
                Some(prep_info.duration_ms),
                custom_vocabulary,
            )
            .await?;

        let active_device = state
            .audio_recorder
            .lock()
            .await
            .active_device_name
            .clone()
            .unwrap_or_else(|| "default microphone".to_string());
        let final_text = validate_transcript(&res.text, prep_info.is_silent, &active_device)?;

        let record = state.repository.complete_transcription(
            &session_id,
            TranscriptionCompletion {
                text: &final_text,
                language: res.detected_language.as_deref(),
                audio_duration_ms,
                processing_duration_ms: res.processing_time_ms as i64,
                engine_id: "mlx-whisper",
                engine_version: "mlx-whisper-0.4.3",
                model_id: &selected_model_id,
                model_revision: MODEL_REVISION,
                model_hash: MODEL_SHA256,
                preprocessing_version: &prep_info.preprocessing_version,
                effective_config_json: "{\"temperature\":0.0,\"no_speech_threshold\":0.8}",
            },
        )?;

        tracing::info!(
            target: "stt_diagnostics",
            session_id = %session_id,
            engine = %selected_engine,
            model = %selected_model_id,
            audio_duration_ms = prep_info.duration_ms,
            processing_time_ms = res.processing_time_ms,
            is_silent = prep_info.is_silent,
            "STT execution completed"
        );

        let auto_copy_setting = state
            .repository
            .get_setting("clipboard.autoCopy")?
            .unwrap_or_default();
        if auto_copy_setting == "true" && !record.text.is_empty() {
            let _ = ClipboardManager::write_text(&record.text);
            let _ = state.repository.mark_copied(&record.id);
        }

        AppResult::Ok(record)
    }
    .await;

    let cleanup_result = remove_runtime_audio(&wav_path);

    match (process_transcription, cleanup_result) {
        (Ok(record), Ok(())) => {
            let payload = state.session_manager.complete_session().await?;
            let _ = app.emit("session-state-changed", &payload);
            let _ = app.emit("transcription-completed", &record);
            Ok(record)
        }
        (Err(err), cleanup_result) if err.code == AppErrorCode::NoSpeechDetected => {
            let deletion_result = state.repository.delete_permanently(&session_id);
            let mut payload = state.session_manager.complete_session().await?;
            let reported_error = cleanup_result.as_ref().err().unwrap_or(&err);
            payload.error_code = Some(reported_error.code.to_string());
            payload.error_message = Some(reported_error.message.clone());
            let _ = app.emit("session-state-changed", &payload);
            deletion_result?;
            cleanup_result?;
            Err(err)
        }
        (Err(err), _) | (_, Err(err)) => {
            tracing::error!("stop_recording failed: {:?}", err);
            if let Ok(payload) = state
                .session_manager
                .fail_session(err.code.clone(), &err.message)
                .await
            {
                let _ = app.emit("session-state-changed", &payload);
            }
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    {
        let mut recorder = state.audio_recorder.lock().await;
        let _ = recorder.stop_recording();
    }

    if let Some(wav_path) = state.record_path_buf.lock().await.take() {
        let _ = remove_runtime_audio(&wav_path);
    }

    if let Some(session_id) = state.record_session_id.lock().await.take() {
        let _ = state.repository.delete_permanently(&session_id);
    }

    let payload = state.session_manager.complete_session().await?;
    let _ = app.emit("session-state-changed", &payload);

    Ok(())
}

#[tauri::command]
pub fn list_transcriptions(
    limit: Option<u32>,
    offset: Option<u32>,
    query: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<TranscriptionRecord>> {
    if let Some(q) = query {
        if !q.trim().is_empty() {
            return state.repository.search(&q);
        }
    }

    state
        .repository
        .list_notes(limit.unwrap_or(50), offset.unwrap_or(0))
}

#[tauri::command]
pub fn update_transcription(
    id: String,
    text: String,
    state: State<'_, AppState>,
) -> AppResult<TranscriptionRecord> {
    state.repository.update_text(&id, &text)
}

#[tauri::command]
pub fn delete_transcription(id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.repository.delete_permanently(&id)
}

fn remove_runtime_audio(path: &std::path::Path) -> AppResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::new(
            AppErrorCode::InternalError,
            format!("Failed to remove transient audio file: {error}"),
        )),
    }
}

fn validate_transcript(text: &str, is_silent: bool, active_device: &str) -> AppResult<String> {
    let text = text.trim();
    if !text.is_empty() {
        return Ok(text.to_string());
    }

    let message = if is_silent {
        format!(
            "No audio was detected from '{active_device}'. Check the selected microphone and try again."
        )
    } else {
        "No speech was detected. Try recording again.".to_string()
    };
    Err(AppError::new(AppErrorCode::NoSpeechDetected, message))
}

async fn emit_idle_error(app: &AppHandle, state: &AppState, error: &AppError) {
    let mut payload = state
        .session_manager
        .transition_to(crate::session::SessionState::Idle)
        .await
        .unwrap_or(SessionStatePayload {
            boot_id: state.boot_id.clone(),
            session_id: None,
            state: crate::session::SessionState::Idle,
            revision: state.session_manager.next_revision(),
            error_code: None,
            error_message: None,
        });
    payload.error_code = Some(error.code.to_string());
    payload.error_message = Some(error.message.clone());
    let _ = app.emit("session-state-changed", &payload);
}

#[tauri::command]
pub fn copy_transcription(
    text: String,
    id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    ClipboardManager::write_text(&text)?;
    if let Some(ref note_id) = id {
        let _ = state.repository.mark_copied(note_id);
    }
    Ok(())
}

#[tauri::command]
pub fn list_input_devices() -> Vec<AudioDeviceInfo> {
    DeviceManager::list_input_devices()
}

#[tauri::command]
pub fn get_settings(key: String, state: State<'_, AppState>) -> AppResult<Option<String>> {
    state.repository.get_setting(&key)
}

#[tauri::command]
pub fn update_settings(
    key: String,
    value_json: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.repository.set_setting(&key, &value_json)
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<ModelInfo> {
    state.model_manager.list_models()
}

#[tauri::command]
pub async fn install_model(
    model_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let app_clone = app.clone();
    let model_mgr = state.model_manager.clone();

    let path = model_mgr
        .download_model(&model_id, move |progress| {
            let _ = app_clone.emit("model-download-progress", &progress);
        })
        .await?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn check_permissions() -> serde_json::Value {
    serde_json::json!({
        "microphone": PermissionManager::check_microphone_permission(),
    })
}

#[tauri::command]
pub async fn request_microphone_permission() -> PermissionStatus {
    PermissionManager::request_microphone_permission().await
}

#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    PermissionManager::open_microphone_settings()
}

#[tauri::command]
pub fn show_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> AppResult<()> {
    show_settings_window(&app)
}

pub fn show_settings_window(app: &AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.unminimize().map_err(|error| {
            AppError::new(
                AppErrorCode::InternalError,
                format!("Failed to restore Settings window: {error}"),
            )
        })?;
        window.show().map_err(|error| {
            AppError::new(
                AppErrorCode::InternalError,
                format!("Failed to show Settings window: {error}"),
            )
        })?;
        window.set_focus().map_err(|error| {
            AppError::new(
                AppErrorCode::InternalError,
                format!("Failed to focus Settings window: {error}"),
            )
        })?;
        return Ok(());
    }

    Err(AppError::new(
        AppErrorCode::InternalError,
        "Settings window is unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transcript_is_trimmed() {
        assert_eq!(
            validate_transcript("  hello from speech notes  ", false, "Built-in Microphone")
                .unwrap(),
            "hello from speech notes"
        );
    }

    #[test]
    fn silent_input_is_a_typed_error_not_transcript_text() {
        let error = validate_transcript("", true, "USB Microphone").unwrap_err();

        assert_eq!(error.code, AppErrorCode::NoSpeechDetected);
        assert!(error.message.contains("USB Microphone"));
    }

    #[test]
    fn no_speech_is_a_typed_error_not_transcript_text() {
        let error = validate_transcript("   ", false, "Built-in Microphone").unwrap_err();

        assert_eq!(error.code, AppErrorCode::NoSpeechDetected);
        assert_eq!(
            error.message,
            "No speech was detected. Try recording again."
        );
    }
}
