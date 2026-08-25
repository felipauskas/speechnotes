# Privacy

## Summary

SpeechNotes is an Apple Silicon macOS 14+ desktop application. It transcribes microphone audio locally using a bundled Python/MLX sidecar and the `mlx-community/whisper-large-v3-mlx` model.

## Data handling

- On first launch, SpeechNotes downloads the pinned transcription model from Hugging Face. That download is subject to Hugging Face's terms and network handling.
- Microphone audio is used at runtime for transcription. It is transient and is deleted after terminal processing paths complete.
- Transcript text and associated dates remain searchable locally until you permanently delete them.
- When optional auto-copy is enabled, SpeechNotes writes the transcript to the macOS clipboard. Other applications with clipboard access may be able to read copied text.
- No telemetry has been identified in SpeechNotes.

## Limits

Deleting audio after processing and permanently deleting a transcript are application-level operations. They do not claim physical erasure from storage media, backups, operating-system caches, swap, or other external systems.

SpeechNotes does not make legal-compliance claims in this document. Your use may be subject to privacy, employment, recording-consent, and other laws that apply to you.
