import type { Machine } from "@everyport/protocol";
import { type CliStatus, type FoundMachine, Onboarding, type OnboardingHost, type OnboardingTool } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { displayShortcut } from "./settings/shortcut";
import { saveConfig, useSettings } from "./settings/useSettings";

const platform: OnboardingHost["platform"] = /Mac/.test(navigator.platform) ? "macos" : /Win/.test(navigator.platform) ? "windows" : "linux";

/** First-run onboarding, answered by the app's Rust side (`onboarding.rs`). */
export function OnboardingWindow() {
  const settings = useSettings();
  const [machines, setMachines] = useState<Machine[]>([]);
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);

  useEffect(() => {
    const off = listen<Machine[]>("machines", ({ payload }) => setMachines(payload));
    void invoke<Machine[]>("machines_list").then(setMachines);
    void invoke<boolean>("launch_at_login").then(setLaunchAtLogin);
    return () => void off.then((f) => f());
  }, []);

  const setLogin = useCallback(async (enabled: boolean) => {
    setLaunchAtLogin(enabled);
    await invoke("set_launch_at_login", { enabled }).catch(() => undefined);
    setLaunchAtLogin(await invoke<boolean>("launch_at_login"));
  }, []);

  // Stable checks, so each step runs its own once.
  const checks = useMemo(
    () => ({
      tools: () => invoke<OnboardingTool[]>("onboarding_tools"),
      cli: () => invoke<CliStatus>("cli_status"),
      installCli: () => invoke<CliStatus>("cli_install"),
      machines: () => invoke<{ found: FoundMachine[] }>("settings_machines").then((list) => list.found),
      openMachineSettings: () => void invoke("open_settings", { pane: "Machines" }),
      finish: () => void invoke("onboarding_finish"),
    }),
    [],
  );

  if (!settings) return null;
  const host: OnboardingHost = {
    ...checks,
    platform,
    shortcut: displayShortcut(settings.app.shortcut),
    launchAtLogin,
    setLaunchAtLogin: (on) => void setLogin(on),
    previews: settings.config.vercel_previews,
    setPreviews: (vercel_previews) => void saveConfig({ vercel_previews }),
  };
  return (
    <Onboarding
      host={host}
      machines={machines}
      theme={settings.app.theme ?? undefined}
      appearance={settings.app.appearance}
      alertMemory={settings.config.alert_memory}
    />
  );
}
