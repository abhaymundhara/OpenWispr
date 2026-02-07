import React, { useEffect, useState } from "react";

function Onboarding() {
  return (
    <div style={{ padding: 20 }}>
      <h1>Welcome to OpenWispr</h1>
      <p>
        Onboarding: follow the steps to download models and grant permissions.
      </p>
      <button id="download-model">Download model (stub)</button>
    </div>
  );
}

export default function App() {
  const [shortcutPressed, setShortcutPressed] = useState(false);

  useEffect(() => {
    // Listen for global shortcut events from the backend
    // Tauri will emit "global-shortcut-pressed"
    // @ts-ignore
    window.addEventListener("global-shortcut-pressed", () => {
      setShortcutPressed(true);
      setTimeout(() => setShortcutPressed(false), 1200);
    });

    // For Tauri event API use window.__TAURI__... in real app
    // This is a minimal stub for the scaffold
  }, []);

  return (
    <div>
      <Onboarding />
      <div style={{ position: "fixed", right: 20, bottom: 20 }}>
        <div
          style={{
            padding: 10,
            background: "#111",
            color: "#fff",
            borderRadius: 8,
          }}
        >
          Global shortcut: {shortcutPressed ? "Pressed ✅" : "Idle"}
        </div>
      </div>
    </div>
  );
}
