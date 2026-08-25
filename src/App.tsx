import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { OverlayView } from "./components/OverlayView";
import { SettingsView } from "./components/SettingsView";
import "./App.css";

function App() {
  const [windowLabel, setWindowLabel] = useState<string>("overlay");

  useEffect(() => {
    try {
      const appWindow = getCurrentWindow();
      setWindowLabel(appWindow.label);
    } catch {
      setWindowLabel("overlay");
    }
  }, []);

  if (windowLabel === "settings") {
    return <SettingsView />;
  }

  return <OverlayView />;
}

export default App;
