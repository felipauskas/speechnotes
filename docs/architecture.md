# SpeechNotes Architecture

SpeechNotes is a local-first macOS menu-bar app for record-then-transcribe notes. It requires Apple Silicon macOS 14+ and uses Whisper Large v3 through Apple MLX.

## Runtime boundaries

1. **React frontend** renders the recording overlay, history, settings, and model-installation progress. It does not access files, databases, or processes directly.
2. **Rust/Tauri core** owns microphone capture, canonical audio validation, session state, model installation and verification, SQLite persistence, optional clipboard delivery, and worker lifecycle.
3. **MLX worker** is a persistent, frozen arm64 sidecar. It receives Protocol v2 NDJSON on stdin, emits responses on stdout, and reserves stderr for diagnostics.
4. **Model store** keeps the pinned Hugging Face MLX model under the application data directory. The required first-run download is staged, SHA-256 verified by Rust, and activated only after verification.

## Transcription flow

1. CPAL records mono 16 kHz signed 16-bit PCM WAV.
2. `AudioPreprocessor` validates the WAV, computes duration and audio SHA-256, and checks for a fully silent signal.
3. `WorkerSupervisor` starts the MLX sidecar, validates Protocol v2, and prepares the local model once.
4. The worker loads the canonical WAV without FFmpeg and transcribes with fixed English settings.
5. A stricter no-speech gate suppresses known silence/noise hallucinations.
6. Rust persists text, engine/model provenance, processing duration, and preprocessing version before optional clipboard actions. Audio is transient; searchable transcript history remains until permanent deletion.

Why this engine: [engine-validation.md](engine-validation.md).

## Worker protocol (NDJSON v2)

One JSON object per line. Stdout is protocol-only; diagnostics use stderr.

```json
{"id":"health-1","action":"ping"}
{"id":"prepare-1","action":"prepare_model","model_dir":"/local/model/directory","model_id":"whisper-large-v3-mlx"}
{"id":"session-id","action":"transcribe","audio_path":"/canonical/16k-mono.wav","language":"en"}
```

`transcribe` may include `initial_prompt` when the user has saved custom vocabulary.

```json
{"id":"health-1","type":"pong","protocol_version":2,"engine":"mlx-whisper","engine_version":"mlx-whisper-0.4.3"}
{"id":"prepare-1","type":"model_ready","model_id":"whisper-large-v3-mlx","load_duration_ms":490}
{"id":"session-id","type":"result","text":"Transcribed text.","detected_language":"en","audio_duration_ms":32213,"processing_time_ms":1036}
{"id":"session-id","type":"error","code":"INVALID_AUDIO","message":"audio must be mono"}
```

The Rust supervisor serializes requests, correlates every response ID, and kills/resets the worker if a request exceeds its timeout. Rust verifies the pinned model download before activation; the worker checks that `weights.npz` exists before loading and does not independently verify its checksum.

## Deployment

- The app bundles a single-file MLX worker; no external Python, Homebrew, or FFmpeg runtime is required.
- Builds are ad-hoc signed, so Gatekeeper may warn.
- Model revision: `49e6aa286ad60c14352c404340ded53710378a11`.
- Model SHA-256: `05ff791ce3630fae47e7c51004e9666204d786246ec07cac6110af768099b40d`.
- The model is downloaded separately because it is approximately 3.1 GB; transcription then operates fully offline.

For a source build of the bundled worker, see [`workers/mlx-whisper-worker/DEVELOPMENT.md`](../workers/mlx-whisper-worker/DEVELOPMENT.md).
