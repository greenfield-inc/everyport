import type { Alert, Server } from "@ppm/protocol";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useFitWindow } from "./fit";

type Notice = { machineId: string; server: Server; alert: Alert };

/** The alert card. The window shows it once it has rendered and sized. */
export function NotificationWindow() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const ref = useFitWindow<HTMLDivElement>();

  useEffect(() => {
    const off = listen<Notice>("notification", ({ payload }) => setNotice(payload));
    void invoke<Notice | null>("notification_current").then((current) => setNotice((n) => n ?? current));
    return () => void off.then((f) => f());
  }, []);

  if (!notice) return null;
  const act = (action: "details" | "stop" | "snooze") => void invoke("notification_action", { action });
  const { server, alert } = notice;
  const gb = (bytes: number) => `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return (
    <div ref={ref} className="window-content">
      <div className="panel notification" role="alert">
        <strong>
          :{server.port} {server.project.name}{" "}
          {alert.kind === "leaking" ? "is leaking" : "is using a lot of memory"}
        </strong>
        <p className="muted">
          {server.project.framework ?? server.process_name} is now using {gb(alert.memory)}.
        </p>
        <div className="actions">
          <button onClick={() => act("details")}>Details</button>
          <button className="destructive" onClick={() => act("stop")}>
            Stop
          </button>
          <button onClick={() => act("snooze")}>Snooze 1h</button>
        </div>
      </div>
    </div>
  );
}
