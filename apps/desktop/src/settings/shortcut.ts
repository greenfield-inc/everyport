/** Global shortcuts in Tauri's accelerator syntax, such as `CommandOrControl+Alt+P`. */

export const DEFAULT_SHORTCUT = "CommandOrControl+Alt+P";

const isMac = navigator.userAgent.includes("Mac");

/** The shortcut a key press makes, or null while only modifiers are down. */
export function fromKeyEvent(event: KeyboardEvent): string | null {
  const key = keyName(event.code);
  if (!key) return null;
  const parts = [];
  if (event.metaKey) parts.push("Super");
  if (event.ctrlKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  return [...parts, key].join("+");
}

/** `KeyP` → `P`, `Digit1` → `1`; other codes, such as `Space` and `F5`, as they are. */
function keyName(code: string): string | null {
  if (/^(Meta|Control|Alt|Shift|OS)(Left|Right)?$/.test(code) || code === "CapsLock") return null;
  return code.replace(/^Key/, "").replace(/^Digit/, "");
}

const MAC_SYMBOLS: Record<string, string> = { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘", CommandOrControl: "⌘" };
const MAC_ORDER = ["Control", "Alt", "Shift", "Super", "CommandOrControl"];
const NAMES: Record<string, string> = { Control: "Ctrl", CommandOrControl: "Ctrl", Super: "Win" };

/** `CommandOrControl+Alt+P` reads `⌥⌘P` on a Mac and `Ctrl+Alt+P` elsewhere. */
export function displayShortcut(shortcut: string): string {
  const parts = shortcut.split("+");
  const key = parts.pop() ?? "";
  if (isMac) {
    const mods = MAC_ORDER.filter((mod) => parts.includes(mod)).map((mod) => MAC_SYMBOLS[mod]);
    return [...new Set(mods)].join("") + key;
  }
  return [...parts.map((mod) => NAMES[mod] ?? mod), key].join("+");
}
