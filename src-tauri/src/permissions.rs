use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
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

    #[test]
    #[ignore = "requires live microphone stream"]
    fn test_cpal_input_stream_capture() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let host = cpal::default_host();
        println!("--- ALL AVAILABLE INPUT DEVICES ---");
        if let Ok(devs) = host.input_devices() {
            for (idx, dev) in devs.enumerate() {
                let name = dev.name().unwrap_or_default();
                println!("Device [{idx}]: {name}");
            }
        }

        let device = host
            .default_input_device()
            .expect("Default input device must exist");
        let dev_name = device.name().unwrap_or_default();
        println!("CPAL Default Input Device: {}", dev_name);

        let config: cpal::StreamConfig = device
            .default_input_config()
            .expect("Default config")
            .into();
        println!(
            "CPAL Config: {} Hz, {} channels",
            config.sample_rate.0, config.channels
        );

        let count = Arc::new(AtomicU32::new(0));
        let count_cb = count.clone();

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _| {
                    count_cb.fetch_add(data.len() as u32, Ordering::Relaxed);
                },
                move |err| {
                    println!("CPAL Stream Error: {:?}", err);
                },
                None,
            )
            .expect("Failed to build input stream");

        stream.play().expect("Failed to play input stream");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let samples_captured = count.load(Ordering::Relaxed);
        println!(
            "Captured {} samples in 300ms from CPAL stream",
            samples_captured
        );
        assert!(
            samples_captured > 0,
            "CPAL input stream should capture samples"
        );
    }
}
