#!/usr/bin/env python3
import gc
import json
import multiprocessing
import os
import sys
import time
import types
import wave

# mlx-whisper imports optional word-timestamp dependencies unconditionally.
# Speech Notes does not request word timestamps, so lightweight stubs avoid
# bundling the otherwise unused SciPy and Numba runtimes.
numba_stub = types.ModuleType("numba")
numba_stub.jit = lambda *args, **kwargs: lambda function: function
signal_stub = types.ModuleType("scipy.signal")
signal_stub.medfilt = lambda *args, **kwargs: (_ for _ in ()).throw(
    RuntimeError("word timestamps are not enabled")
)
scipy_stub = types.ModuleType("scipy")
scipy_stub.signal = signal_stub
sys.modules.setdefault("numba", numba_stub)
sys.modules.setdefault("scipy", scipy_stub)
sys.modules.setdefault("scipy.signal", signal_stub)

import mlx.core as mx
import mlx_whisper
import numpy as np
from mlx_whisper.transcribe import ModelHolder


PROTOCOL_VERSION = 2
ENGINE_VERSION = "mlx-whisper-0.4.3"


def purge_memory():
    """Purges Metal unified memory cache and collects garbage."""
    if hasattr(mx, "clear_cache"):
        mx.clear_cache()
    elif hasattr(mx, "metal") and hasattr(mx.metal, "clear_cache"):
        mx.metal.clear_cache()
    gc.collect()


def send_response(data):
    sys.stdout.write(json.dumps(data, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def send_error(request_id, code, message):
    send_response({
        "id": request_id,
        "type": "error",
        "code": code,
        "message": message,
    })


def load_canonical_wav(path):
    with wave.open(path, "rb") as wav:
        if wav.getnchannels() != 1:
            raise ValueError("audio must be mono")
        if wav.getframerate() != 16000:
            raise ValueError("audio must use a 16000 Hz sample rate")
        if wav.getsampwidth() != 2:
            raise ValueError("audio must use signed 16-bit PCM samples")

        frame_count = wav.getnframes()
        if frame_count == 0:
            return np.zeros(0, dtype=np.float32), 0
        pcm = wav.readframes(frame_count)

    audio = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0
    return audio, int(frame_count / 16000 * 1000)


def main():
    loaded_model_path = None
    loaded_model_id = None

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        request_id = "unknown"
        try:
            try:
                request = json.loads(line)
            except json.JSONDecodeError as error:
                send_error("unknown", "INVALID_JSON", str(error))
                continue

            if not isinstance(request, dict):
                send_error("unknown", "INVALID_REQUEST", "request must be a JSON object")
                continue

            request_id = str(request.get("id", "unknown"))
            action = request.get("action")

            if action == "ping":
                send_response({
                    "id": request_id,
                    "type": "pong",
                    "protocol_version": PROTOCOL_VERSION,
                    "engine": "mlx-whisper",
                    "engine_version": ENGINE_VERSION,
                })
                continue

            if action == "prepare_model":
                model_path = request.get("model_dir")
                model_id = request.get("model_id")
                if not model_path or not os.path.isdir(model_path):
                    send_error(request_id, "MODEL_NOT_FOUND", "model_dir must be an existing local directory")
                    continue

                if loaded_model_path == model_path and ModelHolder.model is not None:
                    send_response({
                        "id": request_id,
                        "type": "model_ready",
                        "model_id": loaded_model_id or model_id or os.path.basename(model_path),
                        "load_duration_ms": 0,
                    })
                    continue

                weights_path = os.path.join(model_path, "weights.npz")
                if not os.path.isfile(weights_path):
                    send_error(request_id, "MODEL_NOT_FOUND", f"weights.npz not found in {model_path}")
                    continue

                started = time.perf_counter()
                try:
                    ModelHolder.get_model(model_path, mx.float16)
                except Exception as error:
                    send_error(request_id, "MODEL_LOAD_FAILED", str(error))
                    continue

                loaded_model_path = model_path
                loaded_model_id = model_id or os.path.basename(model_path)
                send_response({
                    "id": request_id,
                    "type": "model_ready",
                    "model_id": loaded_model_id,
                    "load_duration_ms": int((time.perf_counter() - started) * 1000),
                })
                continue

            if action == "transcribe":
                if loaded_model_path is None:
                    send_error(request_id, "MODEL_NOT_READY", "prepare_model must succeed before transcription")
                    continue

                audio_path = request.get("audio_path")
                if not audio_path or not os.path.isfile(audio_path):
                    send_error(request_id, "AUDIO_NOT_FOUND", "audio_path must be an existing file")
                    continue

                try:
                    audio, audio_duration_ms = load_canonical_wav(audio_path)
                except (ValueError, wave.Error, OSError) as error:
                    send_error(request_id, "INVALID_AUDIO", str(error))
                    continue

                started = time.perf_counter()
                send_response({"id": request_id, "type": "progress", "progress": 0.1})

                # Instant short-circuit on empty or silence audio (RMS < 1e-4)
                if len(audio) == 0 or not np.isfinite(audio).all():
                    send_response({
                        "id": request_id,
                        "type": "result",
                        "text": "",
                        "detected_language": request.get("language", "en") or "en",
                        "audio_duration_ms": 0,
                        "processing_time_ms": int((time.perf_counter() - started) * 1000),
                    })
                    continue

                rms = float(np.sqrt(np.mean(audio**2)))
                if rms < 1e-4:
                    send_response({
                        "id": request_id,
                        "type": "result",
                        "text": "",
                        "detected_language": request.get("language", "en") or "en",
                        "audio_duration_ms": audio_duration_ms,
                        "processing_time_ms": int((time.perf_counter() - started) * 1000),
                    })
                    continue

                initial_prompt = request.get("initial_prompt") or None

                try:
                    result = mlx_whisper.transcribe(
                        audio,
                        path_or_hf_repo=loaded_model_path,
                        language=request.get("language", "en") or "en",
                        task="transcribe",
                        temperature=0.0,
                        fp16=True,
                        logprob_threshold=None,
                        no_speech_threshold=0.8,
                        compression_ratio_threshold=2.4,
                        condition_on_previous_text=False,
                        initial_prompt=initial_prompt,
                        word_timestamps=False,
                    )
                except Exception as error:
                    send_error(request_id, "TRANSCRIPTION_FAILED", str(error))
                    continue
                finally:
                    purge_memory()

                send_response({
                    "id": request_id,
                    "type": "result",
                    "text": result.get("text", "").strip(),
                    "detected_language": result.get("language", "en"),
                    "audio_duration_ms": audio_duration_ms,
                    "processing_time_ms": int((time.perf_counter() - started) * 1000),
                })
                continue

            send_error(request_id, "UNKNOWN_ACTION", f"unsupported action: {action}")

        except Exception as unhandled_error:
            send_error(request_id, "INTERNAL_WORKER_ERROR", str(unhandled_error))


if __name__ == "__main__":
    multiprocessing.freeze_support()
    main()
