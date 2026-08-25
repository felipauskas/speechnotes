use crate::errors::{AppError, AppErrorCode, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use ringbuf::traits::*;
use ringbuf::HeapRb;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use tracing::{error, info};

pub struct AudioRecorder {
    is_recording: Arc<AtomicBool>,
    atomic_level: Arc<AtomicU32>,
    stream: Option<cpal::Stream>,
    writer_handle: Option<thread::JoinHandle<AppResult<u64>>>,
    pub active_device_name: Option<String>,
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for AudioRecorder {}
unsafe impl Sync for AudioRecorder {}

#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0) as i16
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            atomic_level: Arc::new(AtomicU32::new(0)),
            stream: None,
            writer_handle: None,
            active_device_name: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::Relaxed)
    }

    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.atomic_level.load(Ordering::Relaxed))
    }

    pub fn start_recording(
        &mut self,
        output_wav_path: PathBuf,
        preferred_device_name: Option<&str>,
    ) -> AppResult<()> {
        let host = cpal::default_host();

        let device = if let Some(pref_name) =
            preferred_device_name.filter(|s| !s.is_empty() && *s != "System Default")
        {
            if let Ok(devs) = host.input_devices() {
                devs.into_iter()
                    .find(|d| d.name().map(|n| n == pref_name).unwrap_or(false))
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| host.default_input_device())
        .ok_or_else(|| AppError::new(AppErrorCode::NoInputDevice, "No audio input device found"))?;

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Unknown Device".to_string());
        self.active_device_name = Some(device_name.clone());

        let config: cpal::StreamConfig = device
            .default_input_config()
            .map_err(|e| {
                AppError::new(
                    AppErrorCode::AudioCaptureFailed,
                    format!("Failed to get device config for {}: {}", device_name, e),
                )
            })?
            .into();

        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        info!(
            "Starting CPAL audio stream using '{}' at {} Hz, {} channels -> Target 16000 Hz Mono WAV ({:?})",
            device_name, sample_rate, channels, output_wav_path
        );

        // Ring buffer for 30 seconds of source audio
        let rb_size = (sample_rate as usize) * channels * 30;
        let rb = HeapRb::<f32>::new(rb_size);
        let (mut producer, mut consumer) = rb.split();

        let is_recording = Arc::new(AtomicBool::new(true));
        let is_recording_cb = is_recording.clone();
        let atomic_level = Arc::new(AtomicU32::new(0));
        let atomic_level_cb = atomic_level.clone();

        // CPAL Input Callback
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_recording_cb.load(Ordering::Relaxed) {
                        return;
                    }

                    let mut sum_sq = 0.0f32;
                    let mut overflow_count = 0u64;
                    for &sample in data {
                        if producer.try_push(sample).is_err() {
                            overflow_count += 1;
                        }
                        sum_sq += sample * sample;
                    }

                    if overflow_count > 0 {
                        error!(
                            "CPAL ring buffer overflow: dropped {} audio samples",
                            overflow_count
                        );
                    }

                    if !data.is_empty() {
                        let rms = (sum_sq / data.len() as f32).sqrt();
                        atomic_level_cb.store(rms.to_bits(), Ordering::Relaxed);
                    }
                },
                move |err| {
                    error!("CPAL input stream error: {:?}", err);
                },
                None,
            )
            .map_err(|e| AppError::new(AppErrorCode::AudioCaptureFailed, e.to_string()))?;

        stream
            .play()
            .map_err(|e| AppError::new(AppErrorCode::AudioCaptureFailed, e.to_string()))?;

        self.is_recording = is_recording.clone();
        self.atomic_level = atomic_level;
        self.stream = Some(stream);

        // Background Writer & High-Fidelity Sinc Resampler Thread
        let writer_is_recording = is_recording;
        let handle = thread::spawn(move || -> AppResult<u64> {
            let spec = WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let file = File::create(&output_wav_path).map_err(|e| {
                AppError::new(
                    AppErrorCode::AudioCaptureFailed,
                    format!("Failed to create WAV file: {}", e),
                )
            })?;
            let mut writer = WavWriter::new(BufWriter::with_capacity(65536, file), spec)
                .map_err(|e| AppError::new(AppErrorCode::AudioCaptureFailed, e.to_string()))?;

            let chunk_size = 1024;
            let mut resampler = if sample_rate != 16000 {
                let res = FastFixedIn::<f32>::new(
                    16000.0 / sample_rate as f64,
                    2.0,
                    PolynomialDegree::Septic,
                    chunk_size,
                    1,
                )
                .map_err(|e| {
                    AppError::new(
                        AppErrorCode::AudioCaptureFailed,
                        format!("Failed to initialize resampler: {}", e),
                    )
                })?;
                Some(res)
            } else {
                None
            };

            let mut mono_buf: Vec<f32> = Vec::with_capacity(32768);
            let mut raw_samples: Vec<f32> = Vec::with_capacity(8192);
            let mut input_waves = vec![vec![0.0f32; chunk_size]];
            let mut total_samples_written = 0u64;

            while writer_is_recording.load(Ordering::Relaxed) || !consumer.is_empty() {
                raw_samples.clear();

                // Only pop full frames to preserve interleaved channel alignment across pops
                let available = consumer.occupied_len();
                let frame_aligned_count = available - (available % channels);
                let pop_count = frame_aligned_count.min(4096 - (4096 % channels));

                if pop_count > 0 {
                    for _ in 0..pop_count {
                        if let Some(s) = consumer.try_pop() {
                            raw_samples.push(s);
                        }
                    }
                }

                if raw_samples.is_empty() {
                    thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }

                if channels > 1 {
                    for chunk in raw_samples.chunks_exact(channels) {
                        let sum: f32 = chunk.iter().sum();
                        mono_buf.push(sum / channels as f32);
                    }
                } else {
                    mono_buf.extend_from_slice(&raw_samples);
                }

                if let Some(ref mut resampler) = resampler {
                    while mono_buf.len() >= resampler.input_frames_next() {
                        let n_in = resampler.input_frames_next();
                        input_waves[0].resize(n_in, 0.0);
                        input_waves[0].copy_from_slice(&mono_buf[..n_in]);
                        mono_buf.drain(0..n_in);

                        if let Ok(waves_out) = resampler.process(&input_waves, None) {
                            if let Some(out_chan) = waves_out.first() {
                                for &sample in out_chan {
                                    writer.write_sample(f32_to_i16(sample)).ok();
                                    total_samples_written += 1;
                                }
                            }
                        }
                    }
                } else {
                    for &sample in &mono_buf {
                        writer.write_sample(f32_to_i16(sample)).ok();
                        total_samples_written += 1;
                    }
                    mono_buf.clear();
                }
            }

            // Flush trailing audio: drain mono_buf
            if let Some(ref mut resampler) = resampler {
                if !mono_buf.is_empty() {
                    let waves_in = vec![mono_buf];
                    if let Ok(waves_out) = resampler.process_partial(Some(&waves_in), None) {
                        if let Some(out_chan) = waves_out.first() {
                            for &sample in out_chan {
                                writer.write_sample(f32_to_i16(sample)).ok();
                                total_samples_written += 1;
                            }
                        }
                    }
                }
            } else {
                for &sample in &mono_buf {
                    writer.write_sample(f32_to_i16(sample)).ok();
                    total_samples_written += 1;
                }
            }

            writer.finalize().map_err(|e| {
                AppError::new(
                    AppErrorCode::AudioCaptureFailed,
                    format!("Failed to finalize WAV file: {}", e),
                )
            })?;

            Ok(total_samples_written)
        });

        self.writer_handle = Some(handle);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> AppResult<u64> {
        self.is_recording.store(false, Ordering::SeqCst);

        self.stream.take();

        if let Some(handle) = self.writer_handle.take() {
            handle.join().map_err(|_| {
                AppError::new(AppErrorCode::InternalError, "Audio writer thread panicked")
            })?
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rubato_resampling_48k_to_16k() {
        let chunk_size = 1024;
        let mut resampler = FastFixedIn::<f32>::new(
            16000.0 / 48000.0,
            2.0,
            PolynomialDegree::Septic,
            chunk_size,
            1,
        )
        .expect("Failed to create resampler");

        let input_frames = resampler.input_frames_next();
        assert_eq!(input_frames, chunk_size);

        let input_data = vec![vec![0.5f32; input_frames]];
        // Feed two chunks to verify steady-state output
        let _ = resampler.process(&input_data, None).unwrap();
        let output = resampler
            .process(&input_data, None)
            .expect("Failed to process resampling");

        assert_eq!(output.len(), 1);
        let out_samples = &output[0];
        assert!(out_samples.len() > 10);
        // Check steady-state interior samples
        for &s in &out_samples[5..out_samples.len() - 5] {
            assert!((s - 0.5).abs() < 1e-2, "Resampled DC value drifted: {s}");
        }
    }

    #[test]
    fn test_frame_aligned_popping_stereo() {
        let channels = 2;
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, mut consumer) = rb.split();

        // Push 7 samples (3 complete stereo frames + 1 orphan sample)
        // L0=1.0, R0=2.0, L1=3.0, R1=4.0, L2=5.0, R2=6.0, L3=7.0
        for s in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            producer.try_push(s).unwrap();
        }

        let available = consumer.occupied_len();
        let frame_aligned_count = available - (available % channels);
        assert_eq!(frame_aligned_count, 6);

        let mut raw_samples = Vec::new();
        for _ in 0..frame_aligned_count {
            if let Some(s) = consumer.try_pop() {
                raw_samples.push(s);
            }
        }

        assert_eq!(raw_samples.len(), 6);
        assert_eq!(raw_samples, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // The 7th sample remains in consumer for the next full frame pair
        assert_eq!(consumer.occupied_len(), 1);
    }

    #[test]
    fn test_resampler_flushing_recovers_trailing_samples() {
        let chunk_size = 1024;
        let mut resampler = FastFixedIn::<f32>::new(
            16000.0 / 48000.0,
            2.0,
            PolynomialDegree::Septic,
            chunk_size,
            1,
        )
        .unwrap();

        // Feed partial input (e.g. 300 samples of 1.0)
        let partial_input = vec![vec![1.0f32; 300]];
        let partial_out = resampler
            .process_partial(Some(&partial_input), None)
            .unwrap();
        assert_eq!(partial_out.len(), 1);
        let samples = &partial_out[0];
        // Ensure process_partial outputs valid resampled audio
        assert!(!samples.is_empty());
        // The first non-zero resampled sample must reflect the input signal
        assert!((samples[1] - 1.0).abs() < 1e-2);
    }
}
