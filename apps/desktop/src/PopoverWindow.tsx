import { Popover } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { TauriEveryportClient } from "./client";
import { useFitWindow } from "./fit";
import { useSettings } from "./settings/useSettings";
import { installUpdate, useUpdater } from "./updater";

/** Hidden this long, the popover reopens on the list instead of where it was. */
const RESET_AFTER_MS = 60_000;

type ServerRef = { machineId: string; port: number };

export function PopoverWindow({ client }: { client: TauriEveryportClient }) {
  // A new key remounts the view: back to the list, or onto `initialServer`.
  const [view, setView] = useState<{ key: number; initialServer?: ServerRef }>({ key: 0 });
  const ref = useFitWindow<HTMLDivElement>();
  const settings = useSettings();
  const offer = useUpdater()?.offer;

  useEffect(() => {
    let reset: number | undefined;
    const unlisten = [
      listen<boolean>("popover:visible", ({ payload: visible }) => {
        document.documentElement.toggleAttribute("data-hidden", !visible);
        clearTimeout(reset);
        if (!visible) reset = window.setTimeout(() => setView((v) => ({ key: v.key + 1 })), RESET_AFTER_MS);
        // Keys go to the panel; keep focus where it was, or give it to the panel.
        else if (document.activeElement === document.body) document.querySelector<HTMLElement>(".everyport-panel")?.focus();
      }),
      listen<ServerRef>("popover:open-server", ({ payload }) =>
        setView((v) => ({ key: v.key + 1, initialServer: payload })),
      ),
    ];
    // Views that go back on Escape call preventDefault; otherwise it closes.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) void invoke("hide_popover");
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      clearTimeout(reset);
      for (const off of unlisten) void off.then((f) => f());
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  const onReady = useCallback(() => void invoke("popover_ready"), []);

  return (
    <div ref={ref} className="window-content">
      <Popover
        key={view.key}
        client={client}
        initialServer={view.initialServer}
        onReady={onReady}
        theme={settings?.app.theme ?? undefined}
        appearance={settings?.app.appearance}
        alertMemory={settings?.config.alert_memory}
        update={offer ? { version: offer, onUpdate: installUpdate } : undefined}
      />
    </div>
  );
}
