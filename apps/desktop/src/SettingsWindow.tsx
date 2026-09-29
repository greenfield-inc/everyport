import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

const isMac = navigator.userAgent.includes("Mac");

export function SettingsWindow() {
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  useEffect(() => void invoke<boolean>("launch_at_login").then(setLaunchAtLogin), []);

  const toggle = async (enabled: boolean) => {
    setLaunchAtLogin(enabled);
    await invoke("set_launch_at_login", { enabled }).catch(() => undefined);
    setLaunchAtLogin(await invoke<boolean>("launch_at_login"));
  };

  return (
    <main className="settings">
      <h1>General</h1>
      <label>
        <input
          type="checkbox"
          checked={launchAtLogin ?? false}
          disabled={launchAtLogin === null}
          onChange={(event) => void toggle(event.target.checked)}
        />
        Launch at login
      </label>
      <p>
        Open from anywhere with <kbd>{isMac ? "⌥ ⌘ P" : "Ctrl Alt P"}</kbd>
      </p>
    </main>
  );
}
