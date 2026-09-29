// One desktop: the OS's chrome, a wallpaper, and the real @ppm/ui popover
// under (or above) its tray icon.
import type { Machine, Os } from "@ppm/protocol";
import { DotGrid, NotificationCard, Popover } from "@ppm/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import type { DemoClient } from "./client.ts";
import type { View } from "./sections.ts";
import { Terminal } from "./Terminal.tsx";

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 15_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

const format = (now: Date, options: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(undefined, options).format(now).replace(/,/g, "");

const Wifi = () => (
  <svg width="15" height="11" viewBox="0 0 15 11" aria-hidden>
    <path d="M7.5 10.5 5.3 8.1a3.2 3.2 0 0 1 4.4 0Z M1 4.2a9.3 9.3 0 0 1 13 0l-1.4 1.5a7.2 7.2 0 0 0-10.2 0Z M3.2 6.4a6.2 6.2 0 0 1 8.6 0l-1.4 1.4a4.2 4.2 0 0 0-5.8 0Z" fill="currentColor" />
  </svg>
);

const Battery = () => (
  <svg width="22" height="11" viewBox="0 0 22 11" aria-hidden>
    <rect x="0.5" y="0.5" width="18" height="10" rx="3" fill="none" stroke="currentColor" strokeOpacity="0.5" />
    <rect x="2" y="2" width="13" height="7" rx="1.6" fill="currentColor" />
    <path d="M20 3.8v3.4a1.8 1.8 0 0 0 0-3.4Z" fill="currentColor" fillOpacity="0.5" />
  </svg>
);

const Speaker = () => (
  <svg width="14" height="12" viewBox="0 0 14 12" aria-hidden>
    <path d="M1 4h2.5L7 1v10L3.5 8H1Z" fill="currentColor" />
    <path d="M9.2 3.5a3.5 3.5 0 0 1 0 5M11 1.8a6 6 0 0 1 0 8.4" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
  </svg>
);

type Props = {
  os: Os;
  client: DemoClient;
  machine: Machine | undefined;
  view: View;
  open: boolean;
  onToggle: () => void;
  /** Show the leak alert, and what its Details button opens. */
  alert?: { onDetails: (port: number) => void };
  /** Design size in CSS pixels; the page scales the whole screen to fit. */
  width: number;
  height: number;
};

export function Screen({ os, client, machine, view, open, onToggle, alert, width, height }: Props) {
  const now = useClock();
  const servers = machine?.snapshot?.servers ?? [];
  const attention = servers.some((server) => server.status === "attention");
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const off = client.onNotice((text) => {
      clearTimeout(timer);
      setNotice(text);
      timer = setTimeout(() => setNotice(null), 2600);
    });
    return () => {
      off();
      clearTimeout(timer);
    };
  }, [client]);

  const tray = (
    <button type="button" className="tray" data-open={open} onClick={onToggle} aria-label={open ? "Close Port Process Manager" : "Open Port Process Manager"} aria-expanded={open}>
      <DotGrid size={os === "windows" ? 16 : 15} alert={attention} />
      {servers.length > 0 && <span>{servers.length}</span>}
    </button>
  );

  // Snooze hides the alert until the next time the page shows it.
  const [snoozed, setSnoozed] = useState(false);
  const alerting = alert !== undefined;
  useEffect(() => setSnoozed(false), [alerting]);
  const leaking = alert && !snoozed && servers.find((server) => server.status === "attention");
  const compact = width < 600;
  // A phone-sized screen has room for the alert or the popover, not both.
  const showPopover = open && view.kind !== "terminal" && !(compact && leaking);

  return (
    <div className="screen" data-os={os} data-compact={compact || undefined} style={{ width, height }}>
      <div className="wallpaper" aria-hidden />
      {os === "macos" && (
        <div className="menubar">
          <span className="menubar-app">Terminal</span>
          {!compact && (
            <>
              <span>Shell</span>
              <span>Edit</span>
              <span>View</span>
              <span>Window</span>
            </>
          )}
          <span className="spacer" />
          {tray}
          <Wifi />
          <Battery />
          <time>{format(now, compact ? { hour: "numeric", minute: "2-digit" } : { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</time>
        </div>
      )}
      {os === "linux" && (
        <div className="topbar">
          <span className="workspaces" aria-hidden>
            <i />
            <i />
            <i />
          </span>
          <time className="topbar-clock">{format(now, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false })}</time>
          {tray}
          <span className="status-cluster">
            <Wifi />
            <Speaker />
            <Battery />
          </span>
        </div>
      )}
      {os === "windows" && (
        <div className="taskbar">
          <span className="taskbar-apps" aria-hidden>
            <i className="start" />
            <i className="search" />
            {!compact && (
              <>
                <i className="files" />
                <i className="browser" />
              </>
            )}
            <i className="term" />
          </span>
          <span className="taskbar-tray">
            <span className="chevron" aria-hidden>
              ⌃
            </span>
            {tray}
            <span className="status-cluster">
              <Wifi />
              <Speaker />
              <Battery />
            </span>
            <time>
              <span>{format(now, { hour: "numeric", minute: "2-digit" })}</span>
              <span>{format(now, { month: "numeric", day: "numeric", year: "numeric" })}</span>
            </time>
          </span>
        </div>
      )}

      {view.kind === "terminal" && (
        <div className="terminal-slot">
          <Terminal key={machine?.id} client={client} machine={machine} os={os} columns={compact ? 46 : 62} />
        </div>
      )}

      {leaking && (
        <div className="alert-slot">
          <NotificationCard
            appearance="dark"
            server={leaking}
            alert={{ port: leaking.port, kind: "leaking", memory: leaking.memory }}
            onDetails={() => alert.onDetails(leaking.port)}
            onStop={() => void client.call(machine!.id, { method: "stop", params: { port: leaking.port, root: leaking.root, force: false } })}
            onSnooze={() => setSnoozed(true)}
          />
        </div>
      )}

      {showPopover && machine && <PopoverSlot client={client} machine={machine} view={view} />}

      <div className="notice" data-visible={notice !== null} role="status">
        {notice}
      </div>
    </div>
  );
}

function PopoverSlot({ client, machine, view }: { client: DemoClient; machine: Machine; view: View }) {
  const scoped = useMemo(() => client.only(machine.id), [client, machine.id]);
  const slot = useRef<HTMLDivElement>(null);
  const initialServer = view.kind === "detail" ? { machineId: machine.id, port: view.port } : undefined;

  // Clean up opens the way a person opens it: its button on the list.
  const onReady = () => {
    if (view.kind !== "cleanUp") return;
    const button = [...(slot.current?.querySelectorAll("button") ?? [])].find((candidate) => candidate.textContent?.startsWith("Clean up"));
    button?.click();
  };

  return (
    <div className="popover-slot" ref={slot}>
      <Popover key={`${machine.id}-${JSON.stringify(view)}`} client={scoped} appearance="dark" initialServer={initialServer} onReady={onReady} />
    </div>
  );
}
