import { Socket, Themed } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { CleanUpPane } from "./CleanUpPane";
import { GeneralPane } from "./GeneralPane";
import { MachinesPane } from "./MachinesPane";
import "./settings.css";
import { useOnFocus, useSettings } from "./useSettings";

const PANES = [
  { title: "General", view: GeneralPane },
  { title: "Machines", view: MachinesPane, help: "https://github.com/greenfield-inc/everyport/blob/main/docs/machines.md" },
  { title: "Clean up", view: CleanUpPane },
] as const;

/** The Settings window: a sidebar of panes, in the active theme. */
export function SettingsWindow() {
  const settings = useSettings();
  type Pane = (typeof PANES)[number]["title"];
  const [pane, setPane] = useState<Pane>("General");
  // Opened on a pane, such as Machines from onboarding.
  useOnFocus(
    useCallback(() => void invoke<Pane | null>("settings_take_pane").then((next) => next && setPane(next)), []),
  );
  // A save that failed, such as an unwritable settings folder.
  const [failure, setFailure] = useState<string | null>(null);
  useEffect(() => {
    const onFailure = (event: PromiseRejectionEvent) => setFailure(String(event.reason));
    window.addEventListener("unhandledrejection", onFailure);
    return () => window.removeEventListener("unhandledrejection", onFailure);
  }, []);
  useEffect(() => setFailure(null), [settings]);
  if (!settings) return null;
  const current = PANES.find((p) => p.title === pane)!;
  const View = current.view;
  const help = "help" in current ? current.help : undefined;
  return (
    <Themed theme={settings.app.theme ?? undefined} appearance={settings.app.appearance} className="settings">
      <nav className="settings-nav" aria-label="Settings">
        {PANES.map(({ title }) => (
          <button
            key={title}
            type="button"
            aria-current={title === pane ? "page" : undefined}
            onClick={() => setPane(title)}
          >
            <Socket size={14} />
            {title}
          </button>
        ))}
      </nav>
      <main className="settings-pane">
        <h1>
          {pane}
          {help && (
            <button type="button" className="settings-help" aria-label={`About ${pane}`} title="How machines connect" onClick={() => void invoke("open_external", { url: help })}>
              ?
            </button>
          )}
        </h1>
        {[settings.config_error, settings.app_error].filter(Boolean).map((error) => (
          <p key={error} className="settings-error" role="alert">
            Fix this settings file to change settings here. Until then, the last settings that could be read stay in use. {error}
          </p>
        ))}
        {failure && (
          <p className="settings-error" role="alert">
            Couldn't save that change. {failure}
          </p>
        )}
        <View settings={settings} />
      </main>
    </Themed>
  );
}
