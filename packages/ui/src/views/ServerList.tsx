import type { OtherPort, Server } from "@ppm/protocol";
import { useEffect, useRef, useState } from "react";
import { Sparkline } from "../charts.tsx";
import { Colon, Grow, Header, useLeaving } from "../components.tsx";
import type { Pending, ViewContext } from "../context.ts";
import { memory, memoryParts, percent, total, totalParts } from "../format.ts";
import { AgentIcon, BranchIcon, BroomIcon, Chevron, DotGrid, GearIcon, OpenIcon, StopIcon, WorkspaceIcon } from "../icons.tsx";
import { isGone, preselected, rowContext } from "../model.ts";
import { cleanUpCandidates } from "./CleanUp.tsx";

/** At most this many rows show before the list scrolls. */
const VISIBLE_ROWS = 7;
const ROW_HEIGHT = 49;

type Props = {
  ctx: ViewContext;
  /** The keyboard selection. */
  selected: number | null;
  onOpen: (server: Server) => void;
  onCleanUp: () => void;
};

export function ServerList({ ctx, selected, onOpen, onCleanUp }: Props) {
  const { snapshot, client } = ctx;
  const { servers } = snapshot;
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);
  const [segment, setSegment] = useState<number | null>(null);
  const suggested = cleanUpCandidates(servers).filter(preselected).length;

  return (
    <>
      <Summary ctx={ctx} segment={segment} highlighted={hoveredRow} onSegment={setSegment} onOpen={onOpen} />
      <div className="ppm:hairline-t">
        {servers.length ? (
          <Rows ctx={ctx} selected={selected} segment={segment} onHover={setHoveredRow} onOpen={onOpen} />
        ) : (
          <div className="ppm:flex ppm:flex-col ppm:items-center ppm:gap-3 ppm:py-7 ppm:text-center">
            <span className="ppm:text-fg3">
              <DotGrid size={48} />
            </span>
            <span className="ppm:text-13 ppm:text-fg2">Nothing listening</span>
            <span className="ppm:text-11 ppm:text-fg3">Dev servers show up here when they start.</span>
          </div>
        )}
        {snapshot.other_ports.length > 0 && <OtherPorts ports={snapshot.other_ports} />}
      </div>
      <div className="ppm:hairline-t ppm:flex ppm:items-center ppm:justify-between ppm:p-3">
        <button
          type="button"
          onClick={onCleanUp}
          disabled={!servers.length}
          className="ppm:flex ppm:items-center ppm:gap-[7px] ppm:rounded-[7px] ppm:bg-accent ppm:py-[5px] ppm:pr-2.5 ppm:pl-2 ppm:shadow-[inset_0_0.5px_0_color-mix(in_oklab,var(--foreground)_12%,transparent)] ppm:disabled:opacity-50"
        >
          <BroomIcon className="ppm:text-fg" />
          <span className="ppm:text-13 ppm:font-medium ppm:text-fg">Clean up</span>
          {suggested > 0 && <span className="ppm:font-mono ppm:text-11 ppm:font-medium ppm:text-fg2">{suggested}</span>}
        </button>
        {client.openSettings && (
          <button
            type="button"
            aria-label="Settings"
            onClick={() => client.openSettings?.()}
            className="ppm:-m-[5px] ppm:flex ppm:size-[26px] ppm:items-center ppm:justify-center ppm:rounded-[7px] ppm:text-fg2 ppm:hover:bg-accent"
          >
            <GearIcon />
          </button>
        )}
      </div>
    </>
  );
}

function Summary(props: {
  ctx: ViewContext;
  segment: number | null;
  highlighted: number | null;
  onSegment: (port: number | null) => void;
  onOpen: (server: Server) => void;
}) {
  const { servers, system } = props.ctx.snapshot;
  const focused = servers.find((server) => server.port === props.segment);
  const [showMemory, setShowMemory] = useState(false);
  const serversMemory = servers.reduce((sum, server) => sum + server.memory, 0);
  const [amount, unit] = focused ? memoryParts(focused.memory) : totalParts(serversMemory);
  const lit = props.segment ?? props.highlighted;
  const detail = focused
    ? `${percent((focused.memory / system.memory_total) * 100)} of RAM · CPU ${percent(focused.cpu_percent)}`
    : showMemory
      ? `${total(system.memory_other_apps)} other apps · ${total(system.memory_total - system.memory_used)} free`
      : `CPU ${percent(system.cpu_percent)}`;
  return (
    <div className="ppm:flex ppm:flex-col ppm:gap-2.5 ppm:px-4 ppm:pt-3 ppm:pb-4">
      <Header title={focused ? `${focused.project.name} :${focused.port}` : "Servers"} />
      <div className="ppm:flex ppm:flex-col ppm:gap-1.5">
        <div className="ppm:flex ppm:items-center ppm:gap-1.5">
          <div
            className="ppm:flex ppm:items-center ppm:gap-1.5"
            onPointerEnter={() => setShowMemory(true)}
            onPointerLeave={() => setShowMemory(false)}
          >
            <span className="ppm:font-mono ppm:text-28 ppm:font-medium ppm:text-fg">{amount}</span>
            <span className="ppm:self-end ppm:pb-1 ppm:font-mono ppm:text-13 ppm:text-fg3">{unit}</span>
          </div>
          <span className="ppm:ml-auto ppm:grow ppm:text-right ppm:font-mono ppm:text-13 ppm:text-fg3">{detail}</span>
        </div>
        <div className="ppm:flex ppm:h-4 ppm:shrink-0 ppm:items-center" onPointerLeave={() => props.onSegment(null)}>
          <div className="ppm:flex ppm:h-2 ppm:w-full ppm:items-center ppm:gap-0.5" aria-label="Memory by server" role="group">
            {servers.map((server, index) => (
              <button
                key={server.port}
                type="button"
                tabIndex={-1}
                aria-label={`${server.project.name} :${server.port} · ${memory(server.memory)}`}
                onPointerEnter={() => props.onSegment(server.port)}
                onClick={() => props.onOpen(server)}
                className="ppm:h-full ppm:min-w-0.5 ppm:transition-[opacity] ppm:duration-100"
                style={{ flex: `${server.memory} 0 0`, opacity: lit !== null && lit !== server.port ? 0.35 : 1 }}
              >
                <span
                  className="ppm:block ppm:w-full ppm:transition-[height] ppm:duration-100"
                  style={{
                    height: lit === server.port ? 8 : 6,
                    background: server.status === "attention" ? "var(--ppm-warn)" : props.ctx.colorOf(server.port),
                    borderRadius: `${index ? 1 : 3}px ${index === servers.length - 1 ? 3 : 1}px ${index === servers.length - 1 ? 3 : 1}px ${index ? 1 : 3}px`,
                  }}
                />
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function Rows({
  ctx,
  selected,
  segment,
  onHover,
  onOpen,
}: {
  ctx: ViewContext;
  selected: number | null;
  segment: number | null;
  onHover: (port: number | null) => void;
  onOpen: (server: Server) => void;
}) {
  const [hovered, setHovered] = useState<number | null>(null);
  const [live, setLive] = useState(false);
  const shown = useLeaving(ctx.snapshot.servers, (server) => server.port);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => setLive(true), []);
  useEffect(() => {
    list.current?.querySelector(`[data-port="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  return (
    <div
      ref={list}
      role="listbox"
      aria-label="Servers"
      tabIndex={0}
      aria-activedescendant={selected === null ? undefined : `ppm-server-${selected}`}
      data-live={live || undefined}
      className="ppm-scroll ppm:p-1.5"
      style={{ maxHeight: VISIBLE_ROWS * ROW_HEIGHT + 12 }}
      onPointerLeave={() => {
        setHovered(null);
        onHover(null);
      }}
    >
      {shown.map(({ item: server, leaving }) => (
        <Grow key={server.port} leaving={leaving}>
          <Row
            ctx={ctx}
            server={server}
            active={!leaving && (hovered === server.port || selected === server.port)}
            selected={selected === server.port}
            dimmed={segment !== null && segment !== server.port}
            pending={ctx.pending.get(server.port) ?? (leaving ? "stopping" : undefined)}
            onHover={() => {
              setHovered(server.port);
              onHover(server.port);
            }}
            onOpen={() => onOpen(server)}
          />
        </Grow>
      ))}
    </div>
  );
}

function Row({
  ctx,
  server,
  active,
  selected,
  dimmed,
  pending,
  onHover,
  onOpen,
}: {
  ctx: ViewContext;
  server: Server;
  active: boolean;
  selected: boolean;
  dimmed: boolean;
  pending: Pending | undefined;
  onHover: () => void;
  onOpen: () => void;
}) {
  const attention = server.status === "attention";
  const context = rowContext(server, ctx.now, ctx.alertMemory);
  const note =
    pending === "stopping" ? "Stopping…" : pending === "restarting" ? "Restarting…" : (pending?.error ?? null);
  const warn = note ? typeof pending === "object" : context.warn;
  const opacity = dimmed ? 0.35 : pending === "stopping" || isGone(server) ? 0.55 : 1;
  const icon = server.agent ? (
    <AgentIcon kind={server.agent.kind} />
  ) : server.workspace && server.workspace.kind !== "git_worktree" ? (
    <WorkspaceIcon kind={server.workspace.kind} />
  ) : null;

  return (
    <div
      id={`ppm-server-${server.port}`}
      data-port={server.port}
      role="option"
      aria-selected={selected}
      aria-label={`Port ${server.port}, ${server.project.name}, ${memory(server.memory)}`}
      onPointerEnter={onHover}
      onClick={onOpen}
      className={`ppm:flex ppm:items-center ppm:rounded-[9px] ppm:px-2.5 ppm:py-[9px] ppm:transition-opacity ppm:duration-100 ${active ? "ppm:bg-accent" : ""}`}
      style={{ opacity }}
    >
      <Colon status={server.status} color={ctx.colorOf(server.port)} />
      <span className="ppm:w-[46px] ppm:shrink-0 ppm:font-mono ppm:text-13 ppm:font-medium ppm:text-fg">{server.port}</span>
      <div className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:flex-col ppm:gap-px ppm:pr-2.5">
        <div className="ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-1.5 ppm:overflow-clip">
          <span className="ppm:shrink-0 ppm:whitespace-pre ppm:text-13 ppm:font-medium ppm:text-fg">{server.project.name}</span>
          {server.project.branch && (
            <span className="ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-1 ppm:text-fg2">
              <BranchIcon />
              <span className="ppm:clamp-1 ppm:text-13">{server.project.branch}</span>
            </span>
          )}
        </div>
        <div className={`ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-[5px] ppm:text-11 ${warn ? "ppm:text-warn" : "ppm:text-fg2"}`}>
          {!note && !warn && icon}
          <span className="ppm:clamp-1">{note ?? context.text}</span>
        </div>
      </div>
      {active ? (
        // Pointer shortcuts; the keyboard has ⌘O and ⌘⌫, so they stay out of the listbox's tab order.
        <span className="ppm:flex ppm:shrink-0 ppm:items-center ppm:gap-0.5" aria-hidden>
          <button
            type="button"
            tabIndex={-1}
            aria-label="Open in browser"
            onClick={(event) => {
              event.stopPropagation();
              ctx.open(server);
            }}
            className="ppm:flex ppm:size-[22px] ppm:items-center ppm:justify-center ppm:rounded-md ppm:bg-accent ppm:text-fg ppm:hover:bg-[color-mix(in_oklab,var(--accent),var(--foreground)_8%)]"
          >
            <OpenIcon />
          </button>
          <button
            type="button"
            tabIndex={-1}
            aria-label="Stop"
            onClick={(event) => {
              event.stopPropagation();
              ctx.stop(server);
            }}
            className="ppm:flex ppm:size-[22px] ppm:items-center ppm:justify-center ppm:rounded-md ppm:text-fg/80 ppm:hover:bg-accent"
          >
            <StopIcon />
          </button>
        </span>
      ) : (
        <Sparkline history={server.history} warn={attention} />
      )}
      <span className={`ppm:w-[58px] ppm:shrink-0 ppm:text-right ppm:font-mono ppm:text-13 ${attention ? "ppm:text-warn" : "ppm:text-fg/85"}`}>
        {memory(server.memory)}
      </span>
    </div>
  );
}

/** Ports of other users and the system: shown for reference, with no actions. */
function OtherPorts({ ports }: { ports: OtherPort[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="ppm:px-1.5 ppm:pb-1.5">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="ppm:flex ppm:w-full ppm:items-center ppm:gap-1 ppm:rounded-[9px] ppm:px-2.5 ppm:py-[7px] ppm:hover:bg-accent"
      >
        <span className="ppm:text-13 ppm:text-fg2">Other ports</span>
        <span className="ppm:font-mono ppm:text-13 ppm:text-fg3">· {ports.length}</span>
        <Chevron direction={open ? "down" : "right"} className="ppm:text-fg3" />
      </button>
      <div className="ppm-grow" data-closed={!open || undefined} inert={!open}>
        {/* `.ppm-grow > *` clips its child, so the scroller sits one level in. */}
        <div>
          <div className="ppm-scroll" style={{ maxHeight: VISIBLE_ROWS * 26 }}>
          {ports.map((other) => (
            <div
              key={other.port}
              aria-label={`Port ${other.port}${other.process_name ? `, ${other.process_name}` : ""}${other.owner ? `, owned by ${other.owner}` : ""}`}
              className="ppm:flex ppm:items-center ppm:px-2.5 ppm:py-[5px] ppm:text-fg2"
            >
              <span className="ppm:w-[9px] ppm:shrink-0" />
              <span className="selectable ppm:w-[46px] ppm:shrink-0 ppm:font-mono ppm:text-13">{other.port}</span>
              <span className="selectable ppm:clamp-1 ppm:flex-1 ppm:pr-2.5 ppm:text-13">{other.process_name}</span>
              <span className="ppm:shrink-0 ppm:text-11 ppm:text-fg3">{other.owner}</span>
            </div>
          ))}
          </div>
        </div>
      </div>
    </div>
  );
}
