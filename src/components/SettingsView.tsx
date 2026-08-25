import React, { useState, useEffect } from "react";
import { ApiClient } from "../lib/tauri";
import { TranscriptionRecord } from "../lib/generated/TranscriptionRecord";
import { ModelInfo } from "../lib/generated/ModelInfo";
import { AudioDeviceInfo } from "../lib/generated/AudioDeviceInfo";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";

import { PermissionStatus } from "../lib/generated/PermissionStatus";

const PERMISSION_LABELS: Record<PermissionStatus, string> = {
  authorized: "Authorized",
  denied: "Denied",
  restricted: "Restricted",
  notDetermined: "Not Determined",
};

export const SettingsView: React.FC = () => {
  const [transcriptions, setTranscriptions] = useState<TranscriptionRecord[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [modelsLoaded, setModelsLoaded] = useState<boolean>(false);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [selectedDeviceName, setSelectedDeviceName] = useState<string>("System Default");
  const [selectedRecord, setSelectedRecord] = useState<TranscriptionRecord | null>(null);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [downloadProgress, setDownloadProgress] = useState<{ [id: string]: number }>({});
  const [installingModelId, setInstallingModelId] = useState<string | null>(null);
  const [modelSetupMessage, setModelSetupMessage] = useState<string | null>(null);
  const [modelInstallError, setModelInstallError] = useState<string | null>(null);
  const [autostartEnabled, setAutostartEnabled] = useState<boolean>(false);
  const [autoCopyEnabled, setAutoCopyEnabled] = useState<boolean>(false);
  const [micPermission, setMicPermission] = useState<PermissionStatus>("notDetermined");
  const [copiedRecordId, setCopiedRecordId] = useState<string | null>(null);
  const [customVocabulary, setCustomVocabulary] = useState<string>("");
  const [vocabSaved, setVocabSaved] = useState<boolean>(false);

  const loadData = async () => {
    try {
      const perms = await ApiClient.checkPermissions();
      setMicPermission(perms.microphone);

      const autostart = await isEnabled();
      setAutostartEnabled(autostart);

      const autoCopy = await ApiClient.getSetting("clipboard.autoCopy");
      setAutoCopyEnabled(autoCopy === "true");

      const savedVocab = await ApiClient.getSetting("transcription.customVocabulary");
      if (savedVocab) {
        setCustomVocabulary(savedVocab);
      }

      const list = await ApiClient.listTranscriptions(50, 0, searchQuery);
      setTranscriptions(list);
      if (list.length > 0 && !selectedRecord) {
        setSelectedRecord(list[0]);
      }

      const modelList = await ApiClient.listModels();
      setModels(modelList);
      setModelsLoaded(true);
      if (modelList.some((model) => model.is_installed)) {
        setModelSetupMessage(null);
      } else {
        setModelSetupMessage((current) => current || "Install the transcription model before recording.");
      }

      const devList = await ApiClient.listInputDevices();
      setDevices(devList);

      const savedDev = await ApiClient.getSetting("audio.inputDevice");
      if (savedDev) {
        setSelectedDeviceName(savedDev);
      } else {
        const defaultDev = devList.find((d) => d.is_default);
        if (defaultDev) {
          setSelectedDeviceName(defaultDev.name);
        }
      }

    } catch (err) {
      console.error("Failed to load settings data:", err);
    }
  };

  useEffect(() => {
    loadData();

    let isSubscribed = true;
    let unlistenProgress: (() => void) | undefined;

    ApiClient.onModelDownloadProgress((payload) => {
      if (!isSubscribed) return;
      setDownloadProgress((prev) => ({
        ...prev,
        [payload.model_id]: Math.round(payload.progress * 100),
      }));
    }).then((unlisten) => {
      if (!isSubscribed) {
        unlisten();
      } else {
        unlistenProgress = unlisten;
      }
    });

    return () => {
      isSubscribed = false;
      if (unlistenProgress) unlistenProgress();
    };
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      ApiClient.listTranscriptions(50, 0, searchQuery).then(setTranscriptions).catch(console.error);
    }, 200);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  const handleToggleAutostart = async () => {
    try {
      if (autostartEnabled) {
        await disable();
        setAutostartEnabled(false);
      } else {
        await enable();
        setAutostartEnabled(true);
      }
    } catch (err) {
      console.error("Failed to toggle autostart:", err);
    }
  };

  const handleToggleAutoCopy = async () => {
    try {
      const next = !autoCopyEnabled;
      await ApiClient.updateSetting("clipboard.autoCopy", next ? "true" : "false");
      setAutoCopyEnabled(next);
    } catch (err) {
      console.error("Failed to toggle auto-copy:", err);
    }
  };

  const handleRequestMic = async () => {
    await ApiClient.requestMicrophonePermission();
    await loadData();
  };

  const handleInstallModel = async (modelId: string) => {
    try {
      setModelInstallError(null);
      setInstallingModelId(modelId);
      setDownloadProgress((prev) => ({ ...prev, [modelId]: 1 }));
      await ApiClient.installModel(modelId);
      await loadData();
    } catch (err: any) {
      console.error("Failed to install model:", err);
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      setModelInstallError(err?.message || "Model installation failed. Check your connection and try again.");
    } finally {
      setInstallingModelId(null);
    }
  };

  const handleDeleteRecord = async (id: string) => {
    try {
      await ApiClient.deleteTranscription(id);
      if (selectedRecord?.id === id) {
        setSelectedRecord(null);
      }
      await loadData();
    } catch (err) {
      console.error("Failed to delete transcription:", err);
    }
  };

  const handleSaveRecord = async (record: TranscriptionRecord) => {
    try {
      const updated = await ApiClient.updateTranscription(record.id, record.text);
      setSelectedRecord(updated);
      setTranscriptions((prev) =>
        prev.map((item) => (item.id === updated.id ? updated : item))
      );
    } catch (err) {
      console.error("Failed to save transcript:", err);
    }
  };

  const handleCopyRecord = async (record: TranscriptionRecord) => {
    try {
      await ApiClient.updateTranscription(record.id, record.text);
      await ApiClient.copyTranscription(record.text, record.id);
      setCopiedRecordId(record.id);
      setTimeout(() => setCopiedRecordId(null), 1500);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  return (
    <div className="settings-container">
      <header className="settings-header">
        <h1 className="settings-title">Speech Notes — Library & Settings</h1>
        <button className="btn btn-primary" onClick={loadData}>
          Refresh
        </button>
      </header>

      {modelsLoaded && modelSetupMessage && (
        <section className="card" style={{ borderColor: "var(--accent-blue)", marginBottom: "16px" }}>
          <h2 className="card-title">Finish setting up SpeechNotes</h2>
          <p style={{ fontSize: "13px", color: "var(--text-muted)", marginBottom: "12px" }}>
            {modelSetupMessage} The one-time download is 3.1 GB; transcription runs locally after installation.
          </p>
          <button
            className="btn btn-primary"
            onClick={() => document.getElementById("local-speech-models")?.scrollIntoView({ behavior: "smooth", block: "center" })}
          >
            Set up transcription
          </button>
        </section>
      )}

      {/* Transcription Library */}
      <section className="card">
        <h2 className="card-title">Transcription Library</h2>
        <div style={{ marginBottom: "12px" }}>
          <input
            type="text"
            className="transcript-input"
            style={{ height: "38px", padding: "6px 12px" }}
            placeholder="Search transcriptions..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>

        {transcriptions.length === 0 ? (
          <p style={{ fontSize: "13px", color: "var(--text-muted)", padding: "12px 0" }}>
            No transcriptions found. Press <kbd>Ctrl+Shift+Space</kbd> to record a thought.
          </p>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1.2fr", gap: "16px", height: "320px" }}>
            {/* List */}
            <div className="notes-list">
              {transcriptions.map((rec) => (
                <div
                  key={rec.id}
                  className={`note-item ${selectedRecord?.id === rec.id ? "active" : ""}`}
                  style={{
                    cursor: "pointer",
                    borderLeft: selectedRecord?.id === rec.id ? "3px solid var(--accent-blue)" : "1px solid rgba(255, 255, 255, 0.06)",
                  }}
                  onClick={() => setSelectedRecord(rec)}
                >
                  <div className="note-meta">
                    <span>{new Date(Number(rec.created_at)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
                    <span>{(Number(rec.audio_duration_ms || 0) / 1000).toFixed(1)}s</span>
                  </div>
                  <div className="note-text" style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {rec.text || "Empty transcript"}
                  </div>
                </div>
              ))}
            </div>

            {/* Detail Panel */}
            {selectedRecord ? (
              <div className="card" style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", background: "rgba(0,0,0,0.3)" }}>
                <div>
                  <div className="note-meta" style={{ marginBottom: "8px" }}>
                    <span>Created: {new Date(Number(selectedRecord.created_at)).toLocaleString()}</span>
                    <span>
                      Engine: {selectedRecord.engine_id || "Unknown"}
                      {selectedRecord.model_id ? ` / ${selectedRecord.model_id}` : ""}
                    </span>
                  </div>
                  <textarea
                    className="transcript-input"
                    style={{ height: "160px", fontSize: "14px" }}
                    value={selectedRecord.text}
                    onChange={(e) => setSelectedRecord({ ...selectedRecord, text: e.target.value })}
                    onBlur={() => handleSaveRecord(selectedRecord)}
                  />
                </div>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: "12px" }}>
                  <button className="btn" style={{ color: "var(--accent-red)" }} onClick={() => handleDeleteRecord(selectedRecord.id)}>
                    Delete
                  </button>
                  <div style={{ display: "flex", gap: "8px" }}>
                    <button className="btn" onClick={() => handleSaveRecord(selectedRecord)}>
                      Save Edits
                    </button>
                    <button className="btn btn-primary" onClick={() => handleCopyRecord(selectedRecord)}>
                      {copiedRecordId === selectedRecord.id ? "Copied! ✓" : "Copy Text"}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-muted)", fontSize: "13px" }}>
                Select a transcript to view details
              </div>
            )}
          </div>
        )}
      </section>

      {/* Local transcription model */}
      <section id="local-speech-models" className="card">
        <h2 className="card-title">Transcription Model</h2>
        <div className="perm-row" style={{ marginBottom: "12px" }}>
          <div>
            <strong>Transcription Engine</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              MLX Whisper Large v3 transcribes English on Apple silicon and runs fully offline after a one-time 3.1 GB download.
            </p>
          </div>
          <span className="perm-status authorized">MLX Whisper</span>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          {models.map((m) => (
            <div key={m.id} className="perm-row">
              <div>
                <strong>{m.name}</strong>
                <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
                  {m.is_installed ? "Installed and ready for offline English transcription." : "Requires a one-time 3.1 GB download from Hugging Face."}
                </p>
                {downloadProgress[m.id] !== undefined && downloadProgress[m.id] < 100 && (
                  <div style={{ width: "200px", height: "6px", background: "rgba(255,255,255,0.1)", borderRadius: "3px", marginTop: "6px", overflow: "hidden" }}>
                    <div style={{ width: `${downloadProgress[m.id]}%`, height: "100%", background: "var(--accent-blue)" }} />
                  </div>
                )}
              </div>
              <div>
                {m.is_installed ? (
                  <span className="perm-status authorized">Installed</span>
                ) : (
                  <button
                    className="btn btn-primary"
                    disabled={installingModelId === m.id}
                    onClick={() => handleInstallModel(m.id)}
                  >
                    {installingModelId === m.id
                      ? downloadProgress[m.id] !== undefined
                        ? `Installing (${downloadProgress[m.id]}%)`
                        : "Installing..."
                      : "Install Model"}
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
        {modelInstallError && <div className="error-banner" style={{ marginTop: "12px" }}>{modelInstallError}</div>}
      </section>

      {/* Input Device & Permissions */}
      <section className="card">
        <h2 className="card-title">Audio & Microphone Permissions</h2>
        <div className="perm-row">
          <div>
            <strong>Input Microphone</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              Detected system audio input devices.
            </p>
          </div>
          <div>
            <select
              className="btn"
              style={{ padding: "4px 8px" }}
              value={selectedDeviceName}
              onChange={async (e) => {
                const newName = e.target.value;
                setSelectedDeviceName(newName);
                await ApiClient.updateSetting("audio.inputDevice", newName);
              }}
            >
              <option value="System Default">System Default Microphone</option>
              {devices.map((d) => (
                <option key={d.id} value={d.name}>
                  {d.name} {d.is_default ? "(Default)" : ""}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="perm-row">
          <div>
            <strong>Microphone Access</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              Required for on-device voice recording.
            </p>
          </div>
          <div>
            <span className={`perm-status ${micPermission === "authorized" ? "authorized" : "denied"}`}>
              {PERMISSION_LABELS[micPermission] || micPermission}
            </span>
            {micPermission !== "authorized" && (
              <>
                <button className="btn" style={{ marginLeft: "8px" }} onClick={handleRequestMic}>
                  Grant Permission
                </button>
                <button className="btn" style={{ marginLeft: "8px" }} onClick={() => ApiClient.openMicrophoneSettings()}>
                  Open Privacy Settings
                </button>
              </>
            )}
          </div>
        </div>
      </section>

      {/* Custom Vocabulary & Technical Dictionary */}
      <section className="card">
        <h2 className="card-title">Custom Vocabulary & Technical Dictionary</h2>
        <p style={{ fontSize: "12px", color: "var(--text-muted)", marginBottom: "10px" }}>
          Prime Whisper with names, jargon, or acronyms it should recognize (for example product names, people, or team vocabulary).
        </p>
        <textarea
          className="transcript-input"
          style={{ height: "64px", marginBottom: "8px", resize: "vertical", fontSize: "13px" }}
          placeholder="Acme, WidgetCo, Q3 OKRs..."
          value={customVocabulary}
          onChange={(e) => {
            setCustomVocabulary(e.target.value);
            setVocabSaved(false);
          }}
        />
        <div style={{ display: "flex", justifyContent: "flex-end", alignItems: "center", gap: "8px" }}>
          {vocabSaved && <span style={{ fontSize: "12px", color: "var(--accent-green)" }}>Saved to dictionary ✓</span>}
          <button
            className="btn btn-primary"
            onClick={async () => {
              await ApiClient.updateSetting("transcription.customVocabulary", customVocabulary.trim());
              setVocabSaved(true);
              setTimeout(() => setVocabSaved(false), 2500);
            }}
          >
            Save Vocabulary
          </button>
        </div>
      </section>

      {/* Startup & Shortcuts */}
      <section className="card">
        <h2 className="card-title">Startup & Global Shortcuts</h2>
        <div className="perm-row">
          <div>
            <strong>Launch at Login (Autostart)</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              Start Speech Notes silently in the menubar when your Mac boots.
            </p>
          </div>
          <div>
            <button className={`btn ${autostartEnabled ? "btn-primary" : ""}`} onClick={handleToggleAutostart}>
              {autostartEnabled ? "Enabled" : "Enable"}
            </button>
          </div>
        </div>

        <div className="perm-row">
          <div>
            <strong>Copy new transcripts to the clipboard</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              Off by default. When enabled, Speech Notes copies each completed transcript automatically.
            </p>
          </div>
          <div>
            <button className={`btn ${autoCopyEnabled ? "btn-primary" : ""}`} onClick={handleToggleAutoCopy}>
              {autoCopyEnabled ? "Enabled" : "Enable"}
            </button>
          </div>
        </div>

        <div className="perm-row">
          <div>
            <strong>Global Toggle Shortcut</strong>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
              Start and stop voice transcription globally across any app.
            </p>
          </div>
          <div>
            <kbd style={{ background: "rgba(255,255,255,0.1)", padding: "4px 8px", borderRadius: "4px", fontSize: "12px" }}>
              Control + Shift + Space
            </kbd>
          </div>
        </div>
      </section>
    </div>
  );
};
