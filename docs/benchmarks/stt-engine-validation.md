# STT Engine Validation

**Date:** 2026-08-05
**Hardware:** Apple M3 Ultra
**Reference audio:** 32.213-second private held-out recording, manually confirmed to end after “optimal state” followed by silence. The recording and transcript are not included in the repository.

## Accuracy Results

| Candidate | Result at memo ending | Verdict |
|---|---|---|
| MLX Whisper Large v3 Full | Ends correctly at “optimal state.” | Pass |
| MLX Whisper Large v3 Turbo | Ends correctly but differs in punctuation/tokenization. | Near match |
| whisper.cpp Large v3 Full | Adds “I don't know.” | Fail: hallucination |
| whisper.cpp Large v3 Turbo | Adds “All of this is the position.” | Fail: hallucination |
| Distil-Whisper Large v3 | Adds “Thank you.” | Fail: hallucination |
| WhisperKit compressed Large v3 | Duplicates “and” and omits trailing content. | Fail |

Full MLX Large v3 was the only exact match and was selected to preserve maximum accuracy on future difficult speech while remaining comfortably faster than real time.

## Packaged Worker Regression

The frozen arm64 worker was executed through the same typed Rust `WorkerSupervisor` used by Speech Notes.

- Held-out recording: exact confirmed transcript, 32,213 ms duration.
- Known no-speech recording that previously produced “Thank you.”: empty transcript.
- Worker model remains resident across requests.
- No external Python or FFmpeg is used.
- Packaged `.app` resolves the bundled worker from `Contents/MacOS/mlx-whisper-worker`.

## Verification Commands

```bash
npm run build:mlx-worker
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
npm run tauri build -- --debug --bundles app
```

The two real-audio integration tests are ignored by default because private audio and the 3.1 GB model are not repository fixtures. They run when `SPEECH_NOTES_MLX_WORKER`, `SPEECH_NOTES_MLX_MODEL`, `SPEECH_NOTES_TEST_AUDIO`, and `SPEECH_NOTES_SILENCE_AUDIO` are provided.
