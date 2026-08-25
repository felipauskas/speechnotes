# Third-Party Notices

SpeechNotes includes or uses third-party software and a machine-learning model. This is a conservative inventory of major direct components; it may not list every transitive dependency. Users and distributors should review the upstream licenses, notices, and terms for the exact versions they use.

| Component | Purpose | License / terms | Source |
| --- | --- | --- | --- |
| Tauri | Native macOS application framework | MIT OR Apache-2.0 | https://github.com/tauri-apps/tauri |
| Rust | Native application and sidecar components | MIT OR Apache-2.0 | https://www.rust-lang.org/policies/licenses |
| Python | Bundled sidecar runtime | Python Software Foundation License | https://docs.python.org/3/license.html |
| MLX | Apple Silicon model inference | MIT | https://github.com/ml-explore/mlx/blob/main/LICENSE |
| PyInstaller | Python sidecar packaging | GPL-2.0-or-later with a bootloader exception | https://pyinstaller.org/en/stable/license.html |
| mlx-community/whisper-large-v3-mlx | Downloaded transcription model | See the model repository and Hugging Face terms | https://huggingface.co/mlx-community/whisper-large-v3-mlx |
| Whisper | Upstream Whisper model and implementation | MIT | https://github.com/openai/whisper/blob/main/LICENSE |

The model is downloaded on first launch from Hugging Face rather than distributed in the initial app bundle. Its repository card, associated files, and Hugging Face terms govern its use and availability.

Hugging Face terms: https://huggingface.co/terms-of-service
