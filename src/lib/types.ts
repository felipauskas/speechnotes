export type SessionState = "idle" | "recording" | "transcribing";

export type SessionStatePayload = {
  boot_id: string;
  session_id: string | null;
  state: SessionState;
  revision: number;
  error_code: string | null;
  error_message: string | null;
};

export type TranscriptionRecord = {
  id: string;
  created_at: number;
  updated_at: number;
  status: string;
  text: string;
  language: string | null;
  audio_duration_ms: number | null;
  processing_duration_ms: number | null;
  engine_id: string | null;
  engine_version: string | null;
  model_id: string | null;
  error_code: string | null;
  error_message: string | null;
  copied_at: number | null;
};

export type ModelInfo = {
  id: string;
  name: string;
  filename: string;
  url: string;
  expected_sha256: string;
  expected_size_bytes: number;
  is_installed: boolean;
  is_default: boolean;
};

export type DownloadProgressPayload = {
  model_id: string;
  downloaded_bytes: number;
  total_bytes: number;
  progress: number;
};

export type AudioDeviceInfo = {
  id: string;
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
};

export type PermissionStatus = "notDetermined" | "restricted" | "denied" | "authorized";
