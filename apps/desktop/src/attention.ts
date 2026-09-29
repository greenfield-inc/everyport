import { DEFAULT_THEME, warningColor } from "@ppm/ui";
import { invoke } from "@tauri-apps/api/core";

/**
 * Keeps the tray's attention icon in the theme's warning color, for the
 * current light or dark mode. The tray draws in sRGB, so the canvas converts
 * the theme's oklch.
 */
export function syncAttentionColor() {
  const dark = matchMedia("(prefers-color-scheme: dark)");
  const send = () => {
    const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
    context.fillStyle = warningColor(DEFAULT_THEME, dark.matches);
    context.fillRect(0, 0, 1, 1);
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
    void invoke("set_attention_color", { rgb: [r, g, b] });
  };
  send();
  dark.addEventListener("change", send);
}
