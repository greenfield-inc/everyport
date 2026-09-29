import type { Machine, PpmClient, Server, Snapshot } from "@ppm/protocol";
import { useEffect, useRef, useState } from "react";
import { Header } from "./components.tsx";
import { useViewContext, type ViewContext } from "./context.ts";
import { preselected } from "./model.ts";
import { type ThemeProps, Themed } from "./theme.tsx";
import { CleanUp, cleanUpCandidates } from "./views/CleanUp.tsx";
import { MachineStatus, MachineSwitcher } from "./views/MachineSwitcher.tsx";
import { ServerDetail } from "./views/ServerDetail.tsx";
import { ServerList } from "./views/ServerList.tsx";

type Route = { view: "list" } | { view: "detail"; port: number } | { view: "cleanUp" };

export type PopoverProps = ThemeProps & {
  client: PpmClient;
  /** `Config.alert_memory`: the threshold line on memory charts. Defaults to 2 GB. */
  alertMemory?: number;
  /** Open on this server's detail, or the list when it's gone. */
  initialServer?: { machineId: string; port: number };
  /** Called once, after the first render that shows data. */
  onReady?: () => void;
};

/** The whole popover: server list, detail and Clean up, for every machine the client knows. */
export function Popover({ client, alertMemory, initialServer, onReady, theme, appearance }: PopoverProps) {
  const [machines, setMachines] = useState(() => client.machines());
  useEffect(() => client.subscribe(setMachines), [client]);
  const [machineId, setMachineId] = useState(initialServer?.machineId);
  const machine = machines.find((candidate) => candidate.id === machineId) ?? machines[0];

  return (
    <Themed theme={theme} appearance={appearance}>
      <div className="ppm-panel">
        {machines.length > 1 && machine && <MachineSwitcher machines={machines} current={machine.id} onSelect={setMachineId} />}
        {machine?.snapshot ? (
          <MachineView
            key={machine.id}
            client={client}
            machine={machine as Machine & { snapshot: Snapshot }}
            alertMemory={alertMemory}
            initialPort={initialServer?.machineId === machine.id ? initialServer.port : undefined}
            onReady={onReady}
          />
        ) : (
          <div className="flex flex-col">
            <div className="px-4 pt-3 pb-4">
              <Header title="Servers" />
            </div>
            <div className="hairline-t">{machine ? <MachineStatus machine={machine} /> : null}</div>
          </div>
        )}
      </div>
    </Themed>
  );
}

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

function MachineView({
  client,
  machine,
  alertMemory,
  initialPort,
  onReady,
}: {
  client: PpmClient;
  machine: Machine & { snapshot: Snapshot };
  alertMemory?: number;
  initialPort?: number;
  onReady?: () => void;
}) {
  const ctx = useViewContext(client, machine, alertMemory);
  const servers = ctx.snapshot.servers;
  const [route, setRoute] = useState<Route>(initialPort === undefined ? { view: "list" } : { view: "detail", port: initialPort });
  const [direction, setDirection] = useState(1);
  const [selected, setSelected] = useState<number | null>(null);
  const [checked, setChecked] = useState<ReadonlySet<number>>(new Set());
  const transitions = useRef(0);

  const detail = route.view === "detail" ? servers.find((server) => server.port === route.port) : undefined;
  // A detail page whose server stopped falls back to the list.
  const shown: Route = route.view === "detail" && !detail ? { view: "list" } : route;

  const go = (next: Route, dir: 1 | -1) => {
    transitions.current++;
    setDirection(dir);
    if (next.view === "cleanUp") {
      setChecked(new Set(cleanUpCandidates(servers).filter(preselected).map((server) => server.port)));
      setSelected(null);
    }
    if (next.view === "list" && route.view === "detail") setSelected(route.port);
    setRoute(next);
  };
  const back = () => go({ view: "list" }, -1);
  const openDetail = (server: Server) => go({ view: "detail", port: server.port }, 1);
  const toggle = (port: number) =>
    setChecked((current) => {
      const next = new Set(current);
      if (!next.delete(port)) next.add(port);
      return next;
    });

  const ready = useRef(false);
  useEffect(() => {
    if (ready.current) return;
    ready.current = true;
    onReady?.();
  }, [onReady]);

  useKeyboard({ ctx, route: shown, detail, selected, setSelected, go, back, openDetail, toggle });

  const key = shown.view === "detail" ? `detail-${shown.port}` : shown.view;
  return (
    <div
      key={key}
      className="ppm-view"
      data-first={transitions.current === 0 || undefined}
      style={{ ["--ppm-dir" as string]: direction }}
    >
      {shown.view === "detail" && detail ? (
        <ServerDetail ctx={ctx} server={detail} onBack={back} />
      ) : shown.view === "cleanUp" ? (
        <CleanUp ctx={ctx} checked={checked} selected={selected} onToggle={toggle} onBack={back} />
      ) : (
        <ServerList ctx={ctx} selected={selected} onOpen={openDetail} onCleanUp={() => go({ view: "cleanUp" }, 1)} />
      )}
    </div>
  );
}

/** Keyboard use (intent brief, item 8). Keys another handler already took are skipped. */
function useKeyboard(state: {
  ctx: ViewContext;
  route: Route;
  detail: Server | undefined;
  selected: number | null;
  setSelected: (port: number | null) => void;
  go: (route: Route, dir: 1 | -1) => void;
  back: () => void;
  openDetail: (server: Server) => void;
  toggle: (port: number) => void;
}) {
  const latest = useRef(state);
  latest.current = state;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const { ctx, route, detail, selected, setSelected, back, openDetail, toggle } = latest.current;
      const rows = route.view === "cleanUp" ? cleanUpCandidates(ctx.snapshot.servers) : ctx.snapshot.servers;
      const current = rows.find((server) => server.port === selected);
      const target = route.view === "detail" ? detail : route.view === "list" ? current : undefined;
      const mod = isMac ? event.metaKey : event.ctrlKey;
      const handled = () => event.preventDefault();
      // Enter and Space on a focused button press that button.
      const onControl = (event.target as Element | null)?.closest?.("button, [role=menuitem]") != null;

      if (mod) {
        const key = event.key.toLowerCase();
        if (key === "r") handled();
        if (!target) return;
        if (key === "o") {
          handled();
          ctx.open(target);
        } else if (key === "backspace") {
          handled();
          ctx.stop(target);
          if (route.view === "detail") back();
        } else if (key === "r") {
          ctx.restart(target);
        }
        return;
      }

      if (route.view !== "list" && (event.key === "Escape" || event.key === "ArrowLeft")) {
        handled();
        back();
      } else if (route.view !== "detail" && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        handled();
        if (!rows.length) return;
        const index = current ? rows.indexOf(current) : -1;
        const step = event.key === "ArrowDown" ? 1 : -1;
        const next = index === -1 ? (step === 1 ? 0 : rows.length - 1) : Math.min(rows.length - 1, Math.max(0, index + step));
        setSelected(rows[next].port);
      } else if (onControl) {
        return;
      } else if (route.view === "list" && event.key === "Enter" && current) {
        handled();
        openDetail(current);
      } else if (route.view === "cleanUp" && (event.key === "Enter" || event.key === " ") && current) {
        handled();
        toggle(current.port);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
