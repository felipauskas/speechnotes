# SpeechNotes MLX Worker Protocol (NDJSON v2)

SpeechNotes communicates with its persistent MLX sidecar using one JSON object per line. Stdout is protocol-only; diagnostics use stderr.

## Requests

```json
{"id":"health-1","action":"ping"}
```

```json
{"id":"prepare-1","action":"prepare_model","model_dir":"/local/model/directory","model_id":"whisper-large-v3-mlx"}
```

```json
{"id":"session-id","action":"transcribe","audio_path":"/canonical/16k-mono.wav","language":"en"}
```

`transcribe` may include `initial_prompt` when the user has saved custom vocabulary.

## Responses

```json
{"id":"health-1","type":"pong","protocol_version":2,"engine":"mlx-whisper","engine_version":"mlx-whisper-0.4.3"}
```

```json
{"id":"prepare-1","type":"model_ready","model_id":"whisper-large-v3-mlx","load_duration_ms":490}
```

```json
{"id":"session-id","type":"result","text":"Transcribed text.","detected_language":"en","audio_duration_ms":32213,"processing_time_ms":1036}
```

```json
{"id":"session-id","type":"error","code":"INVALID_AUDIO","message":"audio must be mono"}
```

The Rust supervisor serializes requests, correlates every response ID, and kills/resets the worker if a request exceeds its timeout. Rust verifies the pinned model download before activation; the worker checks that `weights.npz` exists before loading and does not independently verify its checksum.
