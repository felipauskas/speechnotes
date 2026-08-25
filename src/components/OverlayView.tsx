import React, { useState, useEffect, useRef } from "react";
import { ApiClient } from "../lib/tauri";
import { SessionState } from "../lib/generated/SessionState";
import { TranscriptionRecord } from "../lib/generated/TranscriptionRecord";

export const OverlayView: React.FC = () => {
  const [sessionState, setSessionState] = useState<SessionState>("idle");
  const [transcriptRecord, setTranscriptRecord] = useState<TranscriptionRecord | null>(null);
  const [transcriptText, setTranscriptText] = useState<string>("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [audioLevel, setAudioLevel] = useState<number>(0);
  const [waveformBars, setWaveformBars] = useState<number[]>(new Array(32).fill(4));
  const [copied, setCopied] = useState<boolean>(false);

  const stateRef = useRef(sessionState);
  stateRef.current = sessionState;

  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Global keydown: Escape cancels recording if active, then hides overlay
  useEffect(() => {
    const handleGlobalKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (stateRef.current === "recording") {
          try {
            await ApiClient.cancelRecording();
          } catch (err) {
            console.error("Failed to cancel recording on Esc:", err);
          }
        }
        ApiClient.hideOverlay().catch(console.error);
      }
    };
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, []);

  // Audio level visualizer listener with StrictMode-safe unlisten
  useEffect(() => {
    let isSubscribed = true;
    let unlistenLevel: (() => void) | undefined;

    ApiClient.onAudioLevelChanged((payload) => {
      if (!isSubscribed) return;
      if (stateRef.current === "recording") {
        const normLevel = Math.min(1.0, Math.max(0, payload.level * 12));
        setAudioLevel(normLevel);
        setWaveformBars((prev) => {
          const next = [...prev.slice(1), Math.max(4, Math.round(normLevel * 40))];
          return next;
        });
      }
    }).then((unlisten) => {
      if (!isSubscribed) {
        unlisten();
      } else {
        unlistenLevel = unlisten;
      }
    });

    return () => {
      isSubscribed = false;
      if (unlistenLevel) unlistenLevel();
    };
  }, []);

  // Session state and transcription delivery listeners
  useEffect(() => {
    let isSubscribed = true;
    let unlistenState: (() => void) | undefined;
    let unlistenCompleted: (() => void) | undefined;

    const setupListeners = async () => {
      try {
        const appState = await ApiClient.getApplicationState();
        if (isSubscribed) {
          setSessionState(appState.state);
        }
      } catch (e) {
        console.error("Failed to get initial application state:", e);
      }

      const unState = await ApiClient.onSessionStateChanged((payload) => {
        if (!isSubscribed) return;
        setSessionState(payload.state);
        if (payload.state === "recording") {
          setErrorMessage(null);
          setTranscriptRecord(null);
          setTranscriptText("");
        }
        if (payload.error_message) {
          setErrorMessage(payload.error_message);
        }
      });
      if (!isSubscribed) unState(); else unlistenState = unState;

      const unCompleted = await ApiClient.onTranscriptionCompleted((record) => {
        if (!isSubscribed) return;
        setTranscriptRecord(record);
        setTranscriptText(record.text);
        setTimeout(() => textareaRef.current?.focus(), 50);
      });
      if (!isSubscribed) unCompleted(); else unlistenCompleted = unCompleted;
    };

    setupListeners();

    return () => {
      isSubscribed = false;
      if (unlistenState) unlistenState();
      if (unlistenCompleted) unlistenCompleted();
    };
  }, []);

  const handleStopRecording = async () => {
    try {
      setErrorMessage(null);
      const record = await ApiClient.stopRecording();
      setTranscriptRecord(record);
      setTranscriptText(record.text);
    } catch (err: any) {
      console.error("Stop recording failed:", err);
      setErrorMessage(err?.message || "Stop failed");
    }
  };

  const handleCancelRecording = async () => {
    try {
      await ApiClient.cancelRecording();
      await ApiClient.hideOverlay();
    } catch (err) {
      console.error("Cancel failed:", err);
    }
  };

  const handleCopy = async () => {
    if (!transcriptText) return;
    try {
      if (transcriptRecord && transcriptText !== transcriptRecord.text) {
        await ApiClient.updateTranscription(transcriptRecord.id, transcriptText);
      }
      await ApiClient.copyTranscription(transcriptText, transcriptRecord?.id);
      setCopied(true);
      setTimeout(async () => {
        setCopied(false);
        await ApiClient.hideOverlay();
      }, 500);
    } catch (err) {
      console.error("Copy failed:", err);
    }
  };

  const handleSave = async () => {
    if (!transcriptRecord || !transcriptText) return;
    try {
      await ApiClient.updateTranscription(transcriptRecord.id, transcriptText);
      await ApiClient.hideOverlay();
    } catch (err) {
      console.error("Update failed:", err);
    }
  };

  const handleDismiss = async () => {
    await ApiClient.hideOverlay();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.metaKey && e.key === "Enter") {
      e.preventDefault();
      handleCopy();
    } else if (e.key === "Escape") {
      e.preventDefault();
      handleDismiss();
    }
  };

  return (
    <div className="hud-container">
      <header className="hud-header" data-tauri-drag-region>
        <div className="brand-section" data-tauri-drag-region>
          <div className={`status-pulse ${sessionState}`} />
          <span className="app-title" data-tauri-drag-region>
            {sessionState === "recording"
              ? "Recording voice (Ctrl+Shift+Space to stop)..."
              : sessionState === "transcribing"
              ? "Transcribing locally (Whisper MLX)..."
              : "Speech Notes"}
          </span>
        </div>
      </header>

      <main className="hud-body">
        {errorMessage && <div className="error-banner">{errorMessage}</div>}

        {sessionState === "recording" && (
          <div style={{ marginBottom: "12px" }}>
            <div className="waveform-visualizer">
              {waveformBars.map((height, i) => (
                <div
                  key={i}
                  className="waveform-bar"
                  style={{ height: `${height}px` }}
                />
              ))}
            </div>
            <div className="level-meter-row">
              <span>Input Level</span>
              <div className="level-bar-track">
                <div
                  className={`level-bar-fill ${audioLevel > 0.85 ? "clipping" : audioLevel < 0.05 ? "silent" : ""}`}
                  style={{ width: `${Math.round(audioLevel * 100)}%` }}
                />
              </div>
              <span className={`signal-badge ${audioLevel > 0.08 ? "healthy" : "silent"}`}>
                {audioLevel > 0.08 ? "Healthy Signal" : "Low / Silent Signal"}
              </span>
            </div>
          </div>
        )}

        <textarea
          ref={textareaRef}
          className="transcript-input"
          placeholder={
            sessionState === "recording"
              ? "Listening... Speak clearly. Press Ctrl+Shift+Space again when done."
              : sessionState === "transcribing"
              ? "Processing audio locally with Apple Silicon GPU acceleration..."
              : "Press Ctrl+Shift+Space to record a thought..."
          }
          value={transcriptText}
          onChange={(e) => setTranscriptText(e.target.value)}
          onKeyDown={handleKeyDown}
          autoFocus
        />
      </main>

      <footer className="hud-footer">
        <div className="shortcut-hints">
          <span className="shortcut-item">
            <kbd>Ctrl+Shift+Space</kbd> Start/Stop
          </span>
          <span className="shortcut-item">
            <kbd>⌘Enter</kbd> Copy
          </span>
          <span className="shortcut-item">
            <kbd>Esc</kbd> Close
          </span>
        </div>

        <div className="action-buttons">
          {sessionState === "recording" ? (
            <>
              <button className="btn" onClick={handleCancelRecording}>
                Cancel
              </button>
              <button className="btn btn-primary" onClick={handleStopRecording}>
                Stop & Transcribe
              </button>
            </>
          ) : (
            <>
              <button className="btn" onClick={handleDismiss}>
                Dismiss
              </button>
              <button className="btn" onClick={handleSave} disabled={!transcriptText}>
                Save Edits
              </button>
              <button className="btn btn-primary" onClick={handleCopy} disabled={!transcriptText || copied}>
                {copied ? "Copied! ✓" : "Copy"}
              </button>
            </>
          )}
        </div>
      </footer>
    </div>
  );
};
