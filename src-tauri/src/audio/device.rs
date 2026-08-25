use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct DeviceManager;

impl DeviceManager {
    pub fn list_input_devices() -> Vec<AudioDeviceInfo> {
        let host = cpal::default_host();
        let default_device_name = host.default_input_device().and_then(|d| d.name().ok());

        let mut devices = Vec::new();
        if let Ok(device_iter) = host.input_devices() {
            for (index, dev) in device_iter.enumerate() {
                if let Ok(name) = dev.name() {
                    let is_default = default_device_name.as_ref() == Some(&name);
                    let (sample_rate, channels) = dev
                        .default_input_config()
                        .map(|c| (c.sample_rate().0, c.channels()))
                        .unwrap_or((16000, 1));

                    devices.push(AudioDeviceInfo {
                        id: format!("dev_{}_{}", index, name.replace(' ', "_")),
                        name,
                        is_default,
                        sample_rate,
                        channels,
                    });
                }
            }
        }

        devices
    }
}
