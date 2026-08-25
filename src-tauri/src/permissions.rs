use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    NotDetermined,
    Restricted,
    Denied,
    Authorized,
}

pub struct PermissionManager;

impl PermissionManager {
    #[cfg(target_os = "macos")]
    pub fn check_microphone_permission() -> PermissionStatus {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
        let status = unsafe {
            let media_type = AVMediaTypeAudio.expect("AVMediaTypeAudio");
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        };

        match status {
            AVAuthorizationStatus::Authorized => PermissionStatus::Authorized,
            AVAuthorizationStatus::Restricted => PermissionStatus::Restricted,
            AVAuthorizationStatus::Denied => PermissionStatus::Denied,
            _ => PermissionStatus::NotDetermined,
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn check_microphone_permission() -> PermissionStatus {
        PermissionStatus::Authorized
    }

    #[cfg(target_os = "macos")]
    pub async fn request_microphone_permission() -> PermissionStatus {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        use std::sync::{Arc, Mutex};
        use tokio::sync::oneshot;

        let current = Self::check_microphone_permission();
        if current == PermissionStatus::Authorized {
            return PermissionStatus::Authorized;
        }

        let (tx, rx) = oneshot::channel::<bool>();
        let tx_cell = Arc::new(Mutex::new(Some(tx)));

        unsafe {
            let block = RcBlock::new(move |granted: Bool| {
                if let Ok(mut lock) = tx_cell.lock() {
                    if let Some(sender) = lock.take() {
                        let _ = sender.send(granted.as_bool());
                    }
                }
            });

            let media_type = AVMediaTypeAudio.expect("AVMediaTypeAudio");
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block);
        }

        match rx.await {
            Ok(true) => PermissionStatus::Authorized,
            _ => Self::check_microphone_permission(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn request_microphone_permission() -> PermissionStatus {
        PermissionStatus::Authorized
    }

    pub fn open_microphone_settings() -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
                .spawn()
                .map_err(|e| format!("Failed to open System Settings: {}", e))?;
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microphone_permission_status_is_a_known_variant() {
        let status = PermissionManager::check_microphone_permission();
        assert!(matches!(
            status,
            PermissionStatus::NotDetermined
                | PermissionStatus::Restricted
                | PermissionStatus::Denied
                | PermissionStatus::Authorized
        ));
    }
}
