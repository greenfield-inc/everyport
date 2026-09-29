import type { Config } from "@ppm/protocol";
import type { Appearance } from "@ppm/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

/** `settings::App` in the app's Rust side, from `app.toml`. */
export type AppSettings = { theme: string | null; appearance: Appearance; shortcut: string };

export type Settings = {
  /** The scanner settings in `config.toml`, which `ppm` also reads. */
  config: Config;
  app: AppSettings;
  /** Why `config.toml` can't be read. Scanner settings don't save until it's fixed. */
  config_error: string | null;
  /** Why `app.toml` can't be read. The app's settings don't save until it's fixed. */
  app_error: string | null;
};

/** The newest settings this page has seen or saved, so quick edits build on each other. */
let latest: Settings | null = null;

/** The current settings, kept live by the `settings` event. Null until loaded. */
export function useSettings(): Settings | null {
  const [settings, setSettings] = useState<Settings | null>(latest);
  useEffect(() => {
    let live = false;
    const set = (next: Settings) => {
      latest = next;
      setSettings(next);
    };
    const off = listen<Settings>("settings", ({ payload }) => {
      live = true;
      set(payload);
    });
    void invoke<Settings>("settings_get").then((current) => {
      if (!live) set(current);
    });
    return () => void off.then((f) => f());
  }, []);
  return settings;
}

/** Saves a change to the scanner settings, which every machine then uses. */
export function saveConfig(change: Partial<Config> | ((config: Config) => Partial<Config>)) {
  const before = latest!;
  const config = { ...before.config, ...(typeof change === "function" ? change(before.config) : change) };
  latest = { ...before, config };
  return invoke<void>("settings_set_config", { config }).catch((error: unknown) => {
    latest = before;
    throw error;
  });
}

/** Saves a change to the app's settings. Rejects with a message when the shortcut can't be used. */
export function saveApp(change: Partial<AppSettings>) {
  const prefs = { ...latest!.app, ...change };
  return invoke<void>("settings_set_app", { prefs }).then(() => {
    latest = { ...latest!, app: prefs };
  });
}

/**
 * Runs `refresh` now and whenever the window comes forward. Settings stays
 * open in the background, and the tray menu or `ppm remote` can change what
 * it shows meanwhile.
 */
export function useOnFocus(refresh: () => void) {
  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);
}
