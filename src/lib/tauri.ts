import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

import {
  AudioDeviceInfo,
  DownloadProgressPayload,
  ModelInfo,
  PermissionStatus,
  SessionStatePayload,
  TranscriptionRecord,
} from "./types";

export class ApiClient {
  static async getApplicationState(): Promise<SessionStatePayload> {
    return await invoke<SessionStatePayload>("get_application_state");
  }

  static async startRecording(): Promise<SessionStatePayload> {
    return await invoke<SessionStatePayload>("start_recording");
  }

  static async stopRecording(): Promise<TranscriptionRecord> {
    return await invoke<TranscriptionRecord>("stop_recording");
  }

  static async cancelRecording(): Promise<void> {
    await invoke<void>("cancel_recording");
  }

  static async toggleRecording(): Promise<SessionStatePayload> {
    return await invoke<SessionStatePayload>("toggle_recording");
  }

  static async listTranscriptions(
    limit: number = 50,
    offset: number = 0,
    query?: string
  ): Promise<TranscriptionRecord[]> {
    return await invoke<TranscriptionRecord[]>("list_transcriptions", { limit, offset, query });
  }

  static async updateTranscription(id: string, text: string): Promise<TranscriptionRecord> {
    return await invoke<TranscriptionRecord>("update_transcription", { id, text });
  }

  static async deleteTranscription(id: string): Promise<void> {
    await invoke<void>("delete_transcription", { id });
  }

  static async copyTranscription(text: string, id?: string): Promise<void> {
    await invoke<void>("copy_transcription", { text, id });
  }

  static async listInputDevices(): Promise<AudioDeviceInfo[]> {
    return await invoke<AudioDeviceInfo[]>("list_input_devices");
  }

  static async getSetting(key: string): Promise<string | null> {
    return await invoke<string | null>("get_settings", { key });
  }

  static async updateSetting(key: string, valueJson: string): Promise<void> {
    await invoke<void>("update_settings", { key, valueJson });
  }

  static async listModels(): Promise<ModelInfo[]> {
    return await invoke<ModelInfo[]>("list_models");
  }

  static async installModel(modelId: string): Promise<string> {
    return await invoke<string>("install_model", { modelId });
  }

  static async checkPermissions(): Promise<{ microphone: PermissionStatus }> {
    return await invoke<{ microphone: PermissionStatus }>("check_permissions");
  }

  static async requestMicrophonePermission(): Promise<PermissionStatus> {
    return await invoke<PermissionStatus>("request_microphone_permission");
  }

  static async openMicrophoneSettings(): Promise<void> {
    await invoke<void>("open_microphone_settings");
  }

  static async showOverlay(): Promise<void> {
    await invoke<void>("show_overlay");
  }

  static async hideOverlay(): Promise<void> {
    await invoke<void>("hide_overlay");
  }

  static async openSettings(): Promise<void> {
    await invoke<void>("open_settings");
  }

  static async onSessionStateChanged(
    cb: (payload: SessionStatePayload) => void
  ): Promise<UnlistenFn> {
    return await listen<SessionStatePayload>("session-state-changed", (e) => cb(e.payload));
  }

  static async onModelDownloadProgress(
    cb: (payload: DownloadProgressPayload) => void
  ): Promise<UnlistenFn> {
    return await listen<DownloadProgressPayload>("model-download-progress", (e) => cb(e.payload));
  }

  static async onAudioLevelChanged(
    cb: (payload: { session_id: string; level: number }) => void
  ): Promise<UnlistenFn> {
    return await listen<{ session_id: string; level: number }>("audio-level-changed", (e) => cb(e.payload));
  }

  static async onTranscriptionCompleted(
    cb: (payload: TranscriptionRecord) => void
  ): Promise<UnlistenFn> {
    return await listen<TranscriptionRecord>("transcription-completed", (e) => cb(e.payload));
  }
}
