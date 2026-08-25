# Changelog

All notable changes to SpeechNotes will be documented in this file.

## 0.1.0 - 2026-08-25

### Added

- Menu-bar dictation on Apple Silicon: `Ctrl+Shift+Space` toggle with live waveform, input level, and silent-signal feedback.
- On-device transcription with an explicitly installed, pinned `whisper-large-v3-mlx` model (SHA-256 verified) served by a PyInstaller-frozen MLX sidecar.
- Transcript library with full-text search, inline editing, per-transcript engine provenance, and permanent delete.
- Transient audio handling: raw recordings are deleted after transcription completes.
- Optional clipboard auto-copy after transcription (off by default).
- Guided first-run microphone permission flow and input device selection.
- Explicit first-run model installation with visible progress, recording readiness checks, and actionable failures.
- Silent and no-speech recordings are discarded instead of entering transcript history or the clipboard.
- Public trust artifacts: LICENSE (MIT), PRIVACY.md, SECURITY.md, THIRD_PARTY_NOTICES.md, CONTRIBUTING.md.
