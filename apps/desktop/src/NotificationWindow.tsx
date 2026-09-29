import type { Alert, Server } from "@everyport/protocol";
import { NotificationCard } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useFitWindow } from "./fit";
import { useSettings } from "./settings/useSettings";

type Notice = { machineId: string; server: Server; alert: Alert };

/** The alert card. The window shows it once it has rendered and sized. */
export function NotificationWindow() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const ref = useFitWindow<HTMLDivElement>();
  const settings = useSettings();

  useEffect(() => {
    const off = listen<Notice>("notification", ({ payload }) => setNotice(payload));
    void invoke<Notice | null>("notification_current").then((current) => setNotice((n) => n ?? current));
    return () => void off.then((f) => f());
  }, []);

  const act = (action: "details" | "stop" | "snooze") => void invoke("notification_action", { action });
  return (
    <div ref={ref} className="window-content">
      {notice && (
        <NotificationCard
          server={notice.server}
          alert={notice.alert}
          theme={settings?.app.theme ?? undefined}
          appearance={settings?.app.appearance}
          alertMemory={settings?.config.alert_memory}
          onDetails={() => act("details")}
          onStop={() => act("stop")}
          onSnooze={() => act("snooze")}
        />
      )}
    </div>
  );
}
