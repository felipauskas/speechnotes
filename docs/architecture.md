# SpeechNotes Architecture

## Overview

SpeechNotes is a local-first macOS menu-bar application for record-then-transcribe notes. It requires Apple Silicon macOS 14+ and uses Whisper Large v3 through Apple MLX.

## Runtime Boundaries

1. **React frontend** renders the recording overlay, history, settings, and model-installation progress. It does not access files, databases, or processes directly.
2. **Rust/Tauri core** owns microphone capture, canonical audio validation, session state, model installation and verification, SQLite persistence, optional clipboard delivery, and worker lifecycle.
3. **MLX worker** is a persistent, frozen arm64 sidecar. It receives Protocol v2 NDJSON on stdin, emits responses on stdout, and reserves stderr for diagnostics.
4. **Model store** keeps the pinned Hugging Face MLX model under the application data directory. The required first-run download is staged, SHA-256 verified by Rust, and activated only after verification.

## Transcription Flow

1. CPAL records mono 16 kHz signed 16-bit PCM WAV.
2. `AudioPreprocessor` validates the WAV, computes duration and audio SHA-256, and checks for a fully silent signal.
3. `WorkerSupervisor` starts the MLX sidecar, validates Protocol v2, and prepares the local model once.
4. The worker loads the canonical WAV without FFmpeg and transcribes with fixed English settings.
5. A stricter no-speech gate suppresses known silence/noise hallucinations.
6. Rust persists text, engine/model provenance, processing duration, and preprocessing version before optional clipboard actions. Audio is transient; searchable transcript history remains until permanent deletion.

## Deployment

- The app bundles a single-file MLX worker; no external Python, Homebrew, or FFmpeg runtime is required.
- The v1 DMG is unsigned or ad-hoc signed, so Gatekeeper may warn.
- Model revision: `49e6aa286ad60c14352c404340ded53710378a11`.
- Model SHA-256: `05ff791ce3630fae47e7c51004e9666204d786246ec07cac6110af768099b40d`.
- The model is downloaded separately because it is approximately 3.1 GB; transcription then operates fully offline.

For a source build of the bundled worker, use the documented procedure in [`workers/mlx-whisper-worker/DEVELOPMENT.md`](../workers/mlx-whisper-worker/DEVELOPMENT.md).
