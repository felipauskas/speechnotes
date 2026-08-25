use crate::persistence::TranscriptionRepository;
use std::path::Path;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

pub struct RecoveryCoordinator;

impl RecoveryCoordinator {
    pub fn reconcile_interrupted_sessions(
        repository: &Arc<TranscriptionRepository>,
        recordings_dir: &Path,
    ) {
        match repository.fail_interrupted_sessions() {
            Ok(0) => {}
            Ok(count) => info!("Reconciled {count} interrupted transcription session(s)"),
            Err(error) => tracing::warn!("Failed to reconcile interrupted sessions: {error}"),
        }
        Self::remove_orphaned_runtime_audio(recordings_dir);
    }

    fn remove_orphaned_runtime_audio(recordings_dir: &Path) {
        let Ok(entries) = std::fs::read_dir(recordings_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let is_runtime_recording = path.is_file()
                && path.extension().is_some_and(|extension| extension == "wav")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| Uuid::parse_str(stem).is_ok());
            if is_runtime_recording {
                if let Err(error) = std::fs::remove_file(&path) {
                    tracing::warn!(?path, %error, "Failed to remove orphaned runtime audio");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_uuid_named_wavs_from_the_runtime_directory() {
        let directory =
            std::env::temp_dir().join(format!("speechnotes-recovery-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let runtime_audio = directory.join(format!("{}.wav", Uuid::new_v4()));
        let unrelated_file = directory.join("keep.txt");
        std::fs::write(&runtime_audio, "audio").unwrap();
        std::fs::write(&unrelated_file, "keep").unwrap();

        RecoveryCoordinator::remove_orphaned_runtime_audio(&directory);

        assert!(!runtime_audio.exists());
        assert!(unrelated_file.exists());
        std::fs::remove_file(unrelated_file).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
