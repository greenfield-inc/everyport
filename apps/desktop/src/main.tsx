import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import "@ppm/ui/styles.css";
import "./app.css";
import { syncAttentionColor } from "./attention";
import { TauriPpmClient } from "./client";
import { NotificationWindow } from "./NotificationWindow";
import { PopoverWindow } from "./PopoverWindow";
import { SettingsWindow } from "./settings/SettingsWindow";

// Right-click opens our own menus or nothing, never the browser's.
document.addEventListener("contextmenu", (event) => event.preventDefault());

const root = document.documentElement;
const label = getCurrentWindow().label;
root.dataset.window = label;

async function page(): Promise<ReactNode> {
  if (label === "settings") return <SettingsWindow />;
  // The popover and notification are transparent over native blur, where there is one.
  root.toggleAttribute("data-vibrancy", await invoke<boolean>("has_vibrancy"));
  if (label === "notification") return <NotificationWindow />;
  root.toggleAttribute("data-hidden", true);
  syncAttentionColor();
  return <PopoverWindow client={new TauriPpmClient()} />;
}

void page().then((content) =>
  createRoot(document.getElementById("root")!).render(<StrictMode>{content}</StrictMode>),
);
