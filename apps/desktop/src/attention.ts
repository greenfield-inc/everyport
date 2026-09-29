import { DEFAULT_THEME, warningColor } from "@ppm/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppSettings, Settings } from "./settings/useSettings";

/**
 * Keeps the tray's attention icon in the chosen theme's warning color, for
 * the current light or dark mode. The tray draws in sRGB, so the canvas
 * converts the theme's oklch.
 */
export function syncAttentionColor() {
  const systemDark = matchMedia("(prefers-color-scheme: dark)");
  let prefs: AppSettings | null = null;
  const send = () => {
    const appearance = prefs?.appearance ?? "system";
    const dark = appearance === "dark" || (appearance === "system" && systemDark.matches);
    const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
    context.fillStyle = warningColor(prefs?.theme ?? DEFAULT_THEME, dark);
    context.fillRect(0, 0, 1, 1);
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
    void invoke("set_attention_color", { rgb: [r, g, b] });
  };
  const update = (settings: Settings) => {
    prefs = settings.app;
    send();
  };
  void invoke<Settings>("settings_get").then(update);
  void listen<Settings>("settings", ({ payload }) => update(payload));
  systemDark.addEventListener("change", send);
}
