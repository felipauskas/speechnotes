# MLX Whisper Worker Development

The frozen sidecar is supported only on Apple Silicon macOS 14 or later.

## Required toolchain

- Xcode Command Line Tools, providing the macOS SDK and `clang`
- An arm64 CPython 3.12 executable available as `python3.12`
- Network access when creating or updating the worker virtual environment, so pip can install `requirements.txt`

Build the sidecar from the repository root:

```sh
npm run build:mlx-worker
```

Set `PYTHON_BIN` only when the arm64 Python 3.12 executable has a different name or path:

```sh
PYTHON_BIN=/path/to/python3.12 npm run build:mlx-worker
```

The build creates ignored `build/`, `dist/`, `.venv/`, and `src-tauri/binaries/` outputs as needed. Its generated PyInstaller spec is also kept in `build/`, leaving the tracked source tree unchanged. It rebuilds when the worker source, dependency manifest, build script, or selected Python/platform changes.
