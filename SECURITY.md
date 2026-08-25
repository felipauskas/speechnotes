# Security

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting form under the repository's **Security** tab. Include a clear description, affected version, reproduction steps, and impact. Do not open a public issue or publish an exploit before maintainers have had a reasonable opportunity to investigate and respond.

## Security notes

- SpeechNotes runs on Apple Silicon Macs with macOS 14 or later.
- It uses a bundled Python/MLX sidecar packaged with PyInstaller for local transcription.
- The user explicitly installs the transcription model from the pinned `mlx-community/whisper-large-v3-mlx` repository on Hugging Face during first-run setup.
- Builds are ad-hoc signed, not notarized. macOS may show installation or execution warnings; build from this repository or obtain binaries only from project-controlled distribution channels.

No software can guarantee complete protection of microphone data, transcripts, or clipboard contents. Keep macOS and SpeechNotes updated, review what you install, and permanently delete sensitive transcripts when they are no longer needed.
