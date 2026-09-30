import type { Alert, Server } from "@everyport/protocol";
import { NotificationCard, UpdateCard } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useFitWindow } from "./fit";
import { useSettings } from "./settings/useSettings";

type Notice = { kind: "alert"; machineId: string; server: Server; alert: Alert } | { kind: "update"; version: string };

/** The alert or update card. The window shows it once it has rendered and sized. */
export function NotificationWindow() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const ref = useFitWindow<HTMLDivElement>();
  const settings = useSettings();

  useEffect(() => {
    const off = listen<Notice>("notification", ({ payload }) => setNotice(payload));
    void invoke<Notice | null>("notification_current").then((current) => setNotice((n) => n ?? current));
    return () => void off.then((f) => f());
  }, []);

  const act = (action: "details" | "stop" | "snooze" | "update" | "skip") => void invoke("notification_action", { action });
  const theme = { theme: settings?.app.theme ?? undefined, appearance: settings?.app.appearance };
  return (
    <div ref={ref} className="window-content">
      {notice?.kind === "alert" && (
        <NotificationCard
          server={notice.server}
          alert={notice.alert}
          {...theme}
          alertMemory={settings?.config.alert_memory}
          onDetails={() => act("details")}
          onStop={() => act("stop")}
          onSnooze={() => act("snooze")}
        />
      )}
      {notice?.kind === "update" && <UpdateCard version={notice.version} {...theme} onUpdate={() => act("update")} onSkip={() => act("skip")} />}
    </div>
  );
}
