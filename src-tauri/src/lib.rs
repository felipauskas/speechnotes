pub mod audio;
pub mod clipboard;
pub mod commands;
pub mod errors;
pub mod models;
pub mod permissions;
pub mod persistence;
pub mod recovery;
pub mod session;
pub mod transcription;

use commands::*;
use models::ModelManager;
use persistence::{Database, TranscriptionRepository};
use recovery::RecoveryCoordinator;
use session::SessionManager;
use transcription::WorkerSupervisor;

use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;
use tracing::error;
use uuid::Uuid;

pub fn run() {
    tracing_subscriber::fmt::init();
    let boot_id = Uuid::new_v4().to_string();

    let app = tauri::Builder::default()
        // Single instance plugin must be registered FIRST
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::AppleScript,
            Some(vec!["--autostart"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app: &AppHandle, shortcut: &Shortcut, event| {
                    let default_shortcut =
                        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

                    if shortcut == &default_shortcut && event.state() == ShortcutState::Pressed {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = app_handle.try_state::<AppState>() {
                                let _ = commands::toggle_recording(app_handle.clone(), state).await;
                            }
                        });
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // App Data Directory Setup
            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
                app.path()
                    .home_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".speechnotes")
            });
            std::fs::create_dir_all(&app_data_dir).ok();
            let db_path = app_data_dir.join("database.sqlite");

            let db = Arc::new(Database::new(db_path).expect("Failed to initialize SQLite DB"));
            let repository = Arc::new(TranscriptionRepository::new(db.clone()));

            // Startup Session Reconciliation
            RecoveryCoordinator::reconcile_interrupted_sessions(
                &repository,
                &app_data_dir.join("recordings"),
            );

            let session_manager =
                Arc::new(SessionManager::new(boot_id.clone(), repository.clone()));

            let models_dir = app_data_dir.join("models");
            let model_manager = Arc::new(ModelManager::new(models_dir));

            let model_is_ready = model_manager.is_model_ready();

            let mut mlx_candidate_paths = Vec::new();
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    mlx_candidate_paths.push(exe_dir.join("mlx-whisper-worker"));
                    mlx_candidate_paths
                        .push(exe_dir.join("mlx-whisper-worker-aarch64-apple-darwin"));
                }
            }
            if let Ok(res_dir) = app.path().resource_dir() {
                mlx_candidate_paths.push(res_dir.join("mlx-whisper-worker"));
                mlx_candidate_paths.push(res_dir.join("mlx-whisper-worker-aarch64-apple-darwin"));
            }
            #[cfg(debug_assertions)]
            mlx_candidate_paths.push(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("binaries/mlx-whisper-worker-aarch64-apple-darwin"),
            );
            let mlx_worker_bin = mlx_candidate_paths
                .into_iter()
                .find(|path| path.exists())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Bundled MLX worker executable was not found",
                    )
                })?;

            tracing::info!("Resolved MLX worker: {:?}", mlx_worker_bin);

            let worker_supervisor = Arc::new(WorkerSupervisor::new(mlx_worker_bin));
            let warm_worker = worker_supervisor.clone();
            let warm_model_manager = model_manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(model_path) = warm_model_manager.get_mlx_model_path() {
                    if let Err(error) = warm_worker
                        .warm_up(&model_path.to_string_lossy(), "whisper-large-v3-mlx")
                        .await
                    {
                        tracing::warn!("MLX worker warm-up failed: {error}");
                    }
                }
            });
            let audio_recorder = Arc::new(Mutex::new(crate::audio::AudioRecorder::new()));

            if let Some(settings_window) = app.get_webview_window("settings") {
                let settings_window_for_close = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Err(error) = settings_window_for_close.hide() {
                            error!("Failed to hide Settings window: {error}");
                        }
                    }
                });
            }

            // Tray Menu
            let record_item = MenuItem::with_id(
                app,
                "record",
                "Start Recording (Ctrl+Shift+Space)",
                true,
                None::<&str>,
            )?;
            let history_item = MenuItem::with_id(
                app,
                "history",
                "Transcripts Library & Settings...",
                true,
                None::<&str>,
            )?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit Speech Notes", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&record_item, &sep, &history_item, &quit_item])?;

            let icon_bytes = include_bytes!("../icons/32x32.png");
            let tray_icon =
                Image::from_bytes(icon_bytes).expect("Failed to load embedded tray icon");

            let tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "record" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = app_handle.try_state::<AppState>() {
                                let _ = commands::toggle_recording(app_handle.clone(), state).await;
                            }
                        });
                    }
                    "history" => {
                        if let Err(error) = commands::show_settings_window(app) {
                            error!("Failed to open Settings window: {error}");
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("overlay") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            app.manage(AppState {
                boot_id,
                session_manager,
                repository,
                model_manager,
                worker_supervisor,
                audio_recorder,
                record_path_buf: Arc::new(Mutex::new(None)),
                record_session_id: Arc::new(Mutex::new(None)),
                tray,
            });

            // Register Default Global Shortcut: Control + Left Shift + Space
            let ctrl_shift_space =
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

            if let Err(e) = app.global_shortcut().register(ctrl_shift_space) {
                error!("Failed to register Ctrl+Shift+Space shortcut: {:?}", e);
            }

            if !model_is_ready {
                if let Err(error) = commands::show_settings_window(app.handle()) {
                    error!("Failed to open model setup window: {error}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_application_state,
            start_recording,
            stop_recording,
            cancel_recording,
            toggle_recording,
            list_transcriptions,
            update_transcription,
            delete_transcription,
            copy_transcription,
            list_input_devices,
            get_settings,
            update_settings,
            list_models,
            install_model,
            check_permissions,
            request_microphone_permission,
            open_microphone_settings,
            show_overlay,
            hide_overlay,
            open_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application");

    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            if let Err(error) = commands::show_settings_window(app) {
                error!("Failed to reopen Settings window: {error}");
            }
        }

        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}
