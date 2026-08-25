#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
WORKER="$SCRIPT_DIR/dist/mlx-whisper-worker"
SIDECAR_DIR="$PROJECT_DIR/src-tauri/binaries"
SIDECAR="$SIDECAR_DIR/mlx-whisper-worker-aarch64-apple-darwin"
STAMP="$SCRIPT_DIR/build-inputs.sha256"
PYTHON_BIN=${PYTHON_BIN:-python3.12}

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
  echo "mlx-whisper-worker must be built on Apple Silicon macOS" >&2
  exit 1
fi

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  echo "Python 3.12 is required; install it or set PYTHON_BIN to an arm64 Python 3.12 executable" >&2
  exit 1
fi

PYTHON_VERSION=$("$PYTHON_BIN" -c 'import platform, sys; print(f"{sys.version_info.major}.{sys.version_info.minor}:{platform.machine()}")')
if [ "$PYTHON_VERSION" != "3.12:arm64" ]; then
  echo "PYTHON_BIN must be an arm64 Python 3.12 executable (got $PYTHON_VERSION)" >&2
  exit 1
fi

BUILD_INPUTS=$(printf '%s\n' \
  "python=$PYTHON_VERSION" \
  "$(shasum -a 256 "$SCRIPT_DIR/mlx_worker.py")" \
  "$(shasum -a 256 "$SCRIPT_DIR/requirements.txt")" \
  "$(shasum -a 256 "$0")" \
  | shasum -a 256 | cut -d ' ' -f 1)

mkdir -p "$SCRIPT_DIR/build" "$SCRIPT_DIR/dist" "$SIDECAR_DIR"

if [ -x "$WORKER" ] && [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$BUILD_INPUTS" ]; then
  cp "$WORKER" "$SIDECAR"
  exit 0
fi

VENV_PYTHON="$SCRIPT_DIR/.venv/bin/python"
if [ ! -x "$VENV_PYTHON" ] || [ "$("$VENV_PYTHON" -c 'import platform, sys; print(f"{sys.version_info.major}.{sys.version_info.minor}:{platform.machine()}")' 2>/dev/null || true)" != "$PYTHON_VERSION" ]; then
  rm -rf "$SCRIPT_DIR/.venv"
  "$PYTHON_BIN" -m venv "$SCRIPT_DIR/.venv"
fi

"$SCRIPT_DIR/.venv/bin/pip" install --requirement "$SCRIPT_DIR/requirements.txt"

"$SCRIPT_DIR/.venv/bin/pyinstaller" \
  --noconfirm \
  --clean \
  --onefile \
  --name mlx-whisper-worker \
  --exclude-module torch \
  --exclude-module scipy \
  --exclude-module numba \
  --exclude-module llvmlite \
  --exclude-module pandas \
  --exclude-module pyarrow \
  --hidden-import termios \
  --collect-data mlx_whisper \
  --collect-all mlx \
  --distpath "$SCRIPT_DIR/dist" \
  --workpath "$SCRIPT_DIR/build" \
  --specpath "$SCRIPT_DIR/build" \
  "$SCRIPT_DIR/mlx_worker.py"

printf '%s\n' "$BUILD_INPUTS" > "$STAMP"
cp "$WORKER" "$SIDECAR"
