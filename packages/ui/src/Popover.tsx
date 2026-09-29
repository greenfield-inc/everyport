import type { Machine, PpmClient, Server, Snapshot } from "@ppm/protocol";
import { type KeyboardEvent, type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Header } from "./components.tsx";
import { Socket } from "./icons.tsx";
import { confirmOf, useViewContext, type ViewContext } from "./context.ts";
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
  /** Move keyboard focus into the popover as it opens. Defaults to true; a page that embeds it passes false. */
  autoFocus?: boolean;
};

/** The whole popover: server list, detail and Clean up, for every machine the client knows. */
export function Popover({ client, alertMemory, initialServer, onReady, autoFocus = true, theme, appearance }: PopoverProps) {
  const [machines, setMachines] = useState(() => client.machines());
  useEffect(() => client.subscribe(setMachines), [client]);
  const [machineId, setMachineId] = useState(initialServer?.machineId);
  const machine = machines.find((candidate) => candidate.id === machineId) ?? machines[0];
  const keys = useRef<((event: KeyboardEvent) => void) | null>(null);

  const ready = useRef(false);
  useEffect(() => {
    if (ready.current || !machine?.snapshot) return;
    ready.current = true;
    onReady?.();
  });

  return (
    <Themed theme={theme} appearance={appearance}>
      <div className="ppm-panel" tabIndex={0} onKeyDown={(event) => keys.current?.(event)}>
        {machines.length > 1 && machine && <MachineSwitcher machines={machines} current={machine.id} onSelect={setMachineId} />}
        {machine?.snapshot ? (
          <MachineView
            key={machine.id}
            client={client}
            machine={machine as Machine & { snapshot: Snapshot }}
            alertMemory={alertMemory}
            initialPort={initialServer?.machineId === machine.id ? initialServer.port : undefined}
            autoFocus={autoFocus}
            keys={keys}
          />
        ) : (
          <div className="ppm:flex ppm:flex-col">
            <div className="ppm:px-4 ppm:pt-3 ppm:pb-4">
              <Header title="Servers" mark={<Socket size={14} />} />
            </div>
            <div className="ppm:hairline-t">{machine ? <MachineStatus machine={machine} client={client} /> : null}</div>
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
  autoFocus,
  keys,
}: {
  client: PpmClient;
  machine: Machine & { snapshot: Snapshot };
  alertMemory?: number;
  initialPort?: number;
  autoFocus: boolean;
  keys: RefObject<((event: KeyboardEvent) => void) | null>;
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

  keys.current = keyHandler({ ctx, route: shown, detail, selected, setSelected, back, openDetail, toggle });

  const key = shown.view === "detail" ? `detail-${shown.port}` : shown.view;

  // Keyboard focus follows the view: the listbox on the list and Clean up, so
  // a screen reader reads the selected row, or the panel on the detail page.
  const view = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!autoFocus && transitions.current === 0) return;
    const root = view.current?.closest<HTMLElement>(".ppm-panel");
    (root?.querySelector<HTMLElement>("[role=listbox]") ?? root)?.focus({ preventScroll: true });
  }, [key]);
  return (
    <div
      key={key}
      ref={view}
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

/**
 * Keyboard use (intent brief, item 8), for keys that reach the panel. Keys
 * typed into a field, or already handled, are left alone.
 */
function keyHandler({
  ctx,
  route,
  detail,
  selected,
  setSelected,
  back,
  openDetail,
  toggle,
}: {
  ctx: ViewContext;
  route: Route;
  detail: Server | undefined;
  selected: number | null;
  setSelected: (port: number | null) => void;
  back: () => void;
  openDetail: (server: Server) => void;
  toggle: (port: number) => void;
}) {
  return (event: KeyboardEvent) => {
    const origin = event.target as HTMLElement;
    if (event.defaultPrevented || origin.closest("input, textarea, select, [contenteditable]:not([contenteditable=false])")) return;
    const rows = route.view === "cleanUp" ? cleanUpCandidates(ctx.snapshot.servers) : ctx.snapshot.servers;
    const current = rows.find((server) => server.port === selected);
    const target = route.view === "detail" ? detail : route.view === "list" ? current : undefined;
    const handled = () => event.preventDefault();

    if (isMac ? event.metaKey : event.ctrlKey) {
      if (!target) return;
      const key = event.key.toLowerCase();
      if (key === "o") {
        handled();
        ctx.open(target);
      } else if (key === "backspace") {
        handled();
        // A held key repeats; only a fresh press may confirm a protected stop.
        if (!event.repeat && ctx.stop(target) && route.view === "detail") back();
      } else if (key === "r") {
        handled();
        if (!event.repeat) ctx.restart(target);
      }
      return;
    }

    if (event.key === "Escape" && [...ctx.pending.values()].some(confirmOf)) {
      handled();
      ctx.cancel();
    } else if (route.view !== "list" && (event.key === "Escape" || event.key === "ArrowLeft")) {
      handled();
      back();
    } else if (route.view !== "detail" && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      handled();
      if (!rows.length) return;
      const index = current ? rows.indexOf(current) : -1;
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = index === -1 ? (step === 1 ? 0 : rows.length - 1) : Math.min(rows.length - 1, Math.max(0, index + step));
      setSelected(rows[next].port);
    } else if (origin.closest("button, [role=menuitem]")) {
      // Enter and Space on a focused button press that button.
    } else if (route.view === "list" && event.key === "Enter" && current) {
      handled();
      openDetail(current);
    } else if (route.view === "cleanUp" && (event.key === "Enter" || event.key === " ") && current) {
      handled();
      toggle(current.port);
    }
  };
}
