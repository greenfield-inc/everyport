import { type Appearance, DEFAULT_THEME, Themed, themeList } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { Row, Section, Segmented, Select, Toggle, count } from "./controls";
import { DEFAULT_SHORTCUT, displayShortcut, fromKeyEvent } from "./shortcut";
import { type Settings, saveApp, saveConfig, useOnFocus } from "./useSettings";

const APPEARANCES = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
] as const satisfies readonly { value: Appearance; label: string }[];

const DOCS_URL = "https://github.com/greenfield-inc/everyport#documentation";

const INTERVALS = [1, 2, 5, 10].map((s) => ({ value: s * 1000, label: count(s, "second") }));

export function GeneralPane({ settings }: { settings: Settings }) {
  const { config, app } = settings;
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  useOnFocus(useCallback(() => void invoke<boolean>("launch_at_login").then(setLaunchAtLogin), []));

  const toggleLogin = async (enabled: boolean) => {
    setLaunchAtLogin(enabled);
    await invoke("set_launch_at_login", { enabled }).catch(() => undefined);
    setLaunchAtLogin(await invoke<boolean>("launch_at_login"));
  };

  return (
    <>
      <Section>
        <Row label="Launch at login">
          {launchAtLogin !== null && <Toggle label="Launch at login" checked={launchAtLogin} onChange={(on) => void toggleLogin(on)} />}
        </Row>
        <Row label="Open Everyport" caption="Works from any app">
          <ShortcutField value={app.shortcut} onChange={(shortcut) => saveApp({ shortcut })} />
        </Row>
        <Row label="Scan every">
          <Select label="Scan every" value={config.interval_ms} options={INTERVALS} onChange={(interval_ms) => void saveConfig({ interval_ms })} />
        </Row>
      </Section>
      <Section title="Integrations">
        <Row label="Vercel previews" caption="Links each branch to its preview. Uses the GitHub CLI (gh), which goes online.">
          <Toggle
            label="Vercel previews"
            checked={config.vercel_previews}
            onChange={(vercel_previews) => void saveConfig({ vercel_previews })}
          />
        </Row>
      </Section>
      <Section title="Appearance">
        <Row label="Mode">
          <Segmented label="Mode" value={app.appearance} options={APPEARANCES} onChange={(appearance) => void saveApp({ appearance })} />
        </Row>
        <ThemePicker
          value={app.theme ?? DEFAULT_THEME}
          appearance={app.appearance}
          onChange={(theme) => void saveApp({ theme })}
        />
      </Section>
      <footer className="settings-footer">
        <button type="button" className="settings-link" onClick={() => void invoke("open_external", { url: DOCS_URL })}>
          Documentation
        </button>
      </footer>
    </>
  );
}

/** Click, then press the new shortcut. Escape cancels. */
function ShortcutField({ value, onChange }: { value: string; onChange: (shortcut: string) => Promise<void> }) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = (shortcut: string) => {
    setRecording(false);
    onChange(shortcut).then(
      () => setError(null),
      (reason: unknown) => setError(String(reason)),
    );
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!recording) return;
    event.preventDefault();
    if (event.key === "Escape") return setRecording(false);
    const shortcut = fromKeyEvent(event.nativeEvent);
    if (shortcut) save(shortcut);
  };

  return (
    <div className="settings-shortcut">
      <button
        type="button"
        className="settings-keys"
        aria-label="Shortcut"
        data-recording={recording || undefined}
        onClick={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        onKeyDown={onKeyDown}
      >
        {recording ? "Press a shortcut" : displayShortcut(value)}
      </button>
      {value !== DEFAULT_SHORTCUT && (
        <button type="button" className="settings-link" onClick={() => save(DEFAULT_SHORTCUT)}>
          Reset
        </button>
      )}
      {error && <small className="settings-error-text">{error}</small>}
    </div>
  );
}

/** Every theme as a small server row drawn in its own colors. Choosing one applies it at once. */
function ThemePicker({ value, appearance, onChange }: { value: string; appearance: Appearance; onChange: (theme: string) => void }) {
  return (
    <div className="settings-themes" role="radiogroup" aria-label="Theme">
      {themeList.map(({ name, title }) => (
        <button key={name} type="button" role="radio" aria-checked={name === value} title={title} onClick={() => onChange(name)}>
          <Themed theme={name} appearance={appearance} className="settings-swatch">
            <span className="settings-swatch-row">
              <span className="settings-swatch-slot" />
              <span className="settings-swatch-port">3000</span>
              <span className="settings-swatch-warn" />
            </span>
            <span className="settings-swatch-bar" />
          </Themed>
          <span className="settings-theme-name">{title}</span>
        </button>
      ))}
    </div>
  );
}
