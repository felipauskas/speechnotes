use crate::errors::{AppError, AppErrorCode, AppResult};
use hound::{SampleFormat, WavReader, WavSpec};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const PREPROCESSING_VERSION: &str = "v1.1.0";

#[derive(Debug, Clone)]
pub struct PreparedAudioInfo {
    pub audio_path: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: u64,
    pub audio_sha256: String,
    pub is_silent: bool,
    pub preprocessing_version: String,
}

pub struct AudioPreprocessor;

impl AudioPreprocessor {
    pub fn prepare_and_validate<P: AsRef<Path>>(path: P) -> AppResult<PreparedAudioInfo> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Err(AppError::new(
                AppErrorCode::AudioCaptureFailed,
                format!("Audio file not found: {:?}", path_ref),
            ));
        }

        let mut reader = WavReader::open(path_ref).map_err(|e| {
            AppError::new(
                AppErrorCode::AudioCaptureFailed,
                format!("Failed to open WAV file {:?}: {}", path_ref, e),
            )
        })?;

        let spec: WavSpec = reader.spec();
        let total_samples = reader.duration() as u64;

        if spec.channels != 1
            || spec.sample_rate != 16_000
            || spec.bits_per_sample != 16
            || spec.sample_format != SampleFormat::Int
        {
            return Err(AppError::new(
                AppErrorCode::AudioCaptureFailed,
                format!(
                    "Audio must be mono 16000 Hz signed PCM16; got {} channel(s), {} Hz, {}-bit {:?}",
                    spec.channels, spec.sample_rate, spec.bits_per_sample, spec.sample_format
                ),
            ));
        }

        if total_samples == 0 {
            return Err(AppError::new(
                AppErrorCode::AudioCaptureFailed,
                format!("Audio file {:?} contains 0 samples", path_ref),
            ));
        }

        let duration_ms = ((total_samples as f64) / (spec.sample_rate as f64) * 1000.0) as u64;

        let mut max_amplitude: i16 = 0;
        let mut hasher = Sha256::new();
        for sample_res in reader.samples::<i16>() {
            let sample = sample_res.map_err(|e| {
                AppError::new(
                    AppErrorCode::AudioCaptureFailed,
                    format!("Failed to read sample: {e}"),
                )
            })?;
            hasher.update(sample.to_le_bytes());
            let abs_sample = sample.saturating_abs();
            if abs_sample > max_amplitude {
                max_amplitude = abs_sample;
            }
        }

        let audio_sha256 = format!("{:x}", hasher.finalize());
        let is_silent = max_amplitude < 50;

        Ok(PreparedAudioInfo {
            audio_path: path_ref.to_string_lossy().to_string(),
            duration_ms,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            total_samples,
            audio_sha256,
            is_silent,
            preprocessing_version: PREPROCESSING_VERSION.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};

    #[test]
    fn test_valid_wav_preprocessing() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_preproc.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(&file_path, spec).unwrap();
        for t in 0..16000 {
            let sample = (0.5
                * (t as f32 * 440.0 * 2.0 * std::f32::consts::PI / 16000.0).sin()
                * 32767.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        let info = AudioPreprocessor::prepare_and_validate(&file_path).unwrap();
        assert_eq!(info.duration_ms, 1000);
        assert_eq!(info.sample_rate, 16000);
        assert_eq!(info.channels, 1);
        assert!(!info.is_silent);
        assert!(!info.audio_sha256.is_empty());

        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_silent_wav_detection() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_silent.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(&file_path, spec).unwrap();
        for _ in 0..8000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let info = AudioPreprocessor::prepare_and_validate(&file_path).unwrap();
        assert_eq!(info.duration_ms, 500);
        assert!(info.is_silent);

        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_rejects_noncanonical_wav() {
        let file_path = std::env::temp_dir().join("test_noncanonical.wav");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(&file_path, spec).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.finalize().unwrap();

        let error = AudioPreprocessor::prepare_and_validate(&file_path).unwrap_err();
        assert_eq!(error.code, AppErrorCode::AudioCaptureFailed);
        assert!(error.message.contains("mono 16000 Hz signed PCM16"));

        std::fs::remove_file(file_path).ok();
    }
}
