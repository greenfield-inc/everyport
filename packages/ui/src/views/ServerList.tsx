import type { OtherPort, Server } from "@everyport/protocol";
import { useEffect, useRef, useState } from "react";
import { Sparkline } from "../charts.tsx";
import { Grow, Header, ProtectedBadge, ProtectedConfirm, StatusSlot, useLeaving, useTween } from "../components.tsx";
import { confirmOf, errorOf, type Pending, type ViewContext } from "../context.ts";
import { memory, memoryParts, percent, total, totalParts } from "../format.ts";
import { AgentIcon, BranchIcon, BroomIcon, Chevron, GearIcon, Socket, OpenIcon, StopIcon, WorkspaceIcon } from "../icons.tsx";
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
      <div className="everyport:hairline-t">
        {servers.length ? (
          <Rows ctx={ctx} selected={selected} segment={segment} onHover={setHoveredRow} onOpen={onOpen} />
        ) : (
          <div className="everyport:flex everyport:flex-col everyport:items-center everyport:gap-3 everyport:py-7 everyport:text-center">
            <span className="everyport:text-fg3">
              <Socket size={48} />
            </span>
            <span className="everyport:text-13 everyport:text-fg2">Nothing listening</span>
            <span className="everyport:text-11 everyport:text-fg3">Dev servers show up here when they start.</span>
          </div>
        )}
        {snapshot.other_ports.length > 0 && <OtherPorts ports={snapshot.other_ports} />}
      </div>
      <div className="everyport:hairline-t everyport:flex everyport:items-center everyport:justify-between everyport:p-3">
        <button
          type="button"
          onClick={onCleanUp}
          disabled={!servers.length}
          className="everyport:flex everyport:items-center everyport:gap-[7px] everyport:rounded-[7px] everyport:bg-accent everyport:py-[5px] everyport:pr-2.5 everyport:pl-2 everyport:shadow-[inset_0_0.5px_0_color-mix(in_oklab,var(--foreground)_12%,transparent)] everyport:disabled:opacity-50"
        >
          <BroomIcon className="everyport:text-fg" />
          <span className="everyport:text-13 everyport:font-medium everyport:text-fg">Clean up</span>
          {suggested > 0 && <span className="everyport:font-mono everyport:text-11 everyport:font-medium everyport:text-fg2">{suggested}</span>}
        </button>
        {client.openSettings && (
          <button
            type="button"
            aria-label="Settings"
            onClick={() => client.openSettings?.()}
            className="everyport:-m-[5px] everyport:flex everyport:size-[26px] everyport:items-center everyport:justify-center everyport:rounded-[7px] everyport:text-fg2 everyport:hover:bg-accent"
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
  const serversMemory = useTween(servers.reduce((sum, server) => sum + server.memory, 0));
  const [amount, unit] = focused ? memoryParts(focused.memory) : totalParts(serversMemory);
  const lit = props.segment ?? props.highlighted;
  const detail = focused
    ? `${percent((focused.memory / system.memory_total) * 100)} of RAM · CPU ${percent(focused.cpu_percent)}`
    : showMemory
      ? `${total(system.memory_other_apps)} other apps · ${total(system.memory_total - system.memory_used)} free`
      : `CPU ${percent(system.cpu_percent)}`;
  return (
    <div className="everyport:flex everyport:flex-col everyport:gap-2.5 everyport:px-4 everyport:pt-3 everyport:pb-4">
      <Header
        title={focused ? `${focused.project.name} :${focused.port}` : "Servers"}
        mark={<Socket size={14} state={servers.some((s) => s.status === "attention") ? "attention" : servers.length ? "running" : "idle"} />}
      />
      <div className="everyport:flex everyport:flex-col everyport:gap-1.5">
        <div className="everyport:flex everyport:items-center everyport:gap-1.5">
          <div
            className="everyport:flex everyport:items-center everyport:gap-1.5"
            onPointerEnter={() => setShowMemory(true)}
            onPointerLeave={() => setShowMemory(false)}
          >
            <span className="everyport:font-mono everyport:text-28 everyport:font-medium everyport:text-fg">{amount}</span>
            <span className="everyport:self-end everyport:pb-1 everyport:font-mono everyport:text-13 everyport:text-fg3">{unit}</span>
          </div>
          <span className="everyport:ml-auto everyport:grow everyport:text-right everyport:font-mono everyport:text-13 everyport:text-fg3">{detail}</span>
        </div>
        <div className="everyport:flex everyport:h-4 everyport:shrink-0 everyport:items-center" onPointerLeave={() => props.onSegment(null)}>
          <div className="everyport:flex everyport:h-2 everyport:w-full everyport:items-center everyport:gap-0.5" aria-label="Memory by server" role="group">
            {servers.map((server, index) => (
              <button
                key={server.port}
                type="button"
                tabIndex={-1}
                aria-label={`${server.project.name} :${server.port} · ${memory(server.memory)}`}
                onPointerEnter={() => props.onSegment(server.port)}
                onClick={() => props.onOpen(server)}
                className="everyport:h-full everyport:min-w-0.5 everyport:transition-[opacity] everyport:duration-100"
                style={{ flex: `${server.memory} 0 0`, opacity: lit !== null && lit !== server.port ? 0.35 : 1 }}
              >
                <span
                  className="everyport:block everyport:w-full everyport:transition-[height] everyport:duration-100"
                  style={{
                    height: lit === server.port ? 8 : 6,
                    background: server.status === "attention" ? "var(--everyport-warn)" : props.ctx.colorOf(server.port),
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
      aria-activedescendant={selected === null ? undefined : `everyport-server-${selected}`}
      data-live={live || undefined}
      className="everyport-scroll everyport:p-1.5"
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
  const confirm = confirmOf(pending);
  const error = errorOf(pending);
  const note = pending === "stopping" ? "Stopping…" : pending === "restarting" ? "Restarting…" : error;
  const warn = note ? error !== null : context.warn;
  const opacity = dimmed ? 0.35 : pending === "stopping" || isGone(server) ? 0.55 : 1;
  const icon = server.agent ? (
    <AgentIcon kind={server.agent.kind} />
  ) : server.workspace && server.workspace.kind !== "git_worktree" ? (
    <WorkspaceIcon kind={server.workspace.kind} />
  ) : null;

  return (
    <div
      id={`everyport-server-${server.port}`}
      data-port={server.port}
      role="option"
      aria-selected={selected}
      aria-label={`Port ${server.port}, ${server.project.name}, ${memory(server.memory)}`}
      onPointerEnter={onHover}
      onClick={onOpen}
      className={`everyport:flex everyport:items-center everyport:rounded-[9px] everyport:px-2.5 everyport:py-[9px] everyport:transition-opacity everyport:duration-100 ${active ? "everyport:bg-accent" : ""}`}
      style={{ opacity }}
    >
      <StatusSlot status={server.status} color={ctx.colorOf(server.port)} />
      <span className="everyport:w-[46px] everyport:shrink-0 everyport:font-mono everyport:text-13 everyport:font-medium everyport:text-fg">{server.port}</span>
      {confirm ? (
        <ProtectedConfirm ctx={ctx} server={server} action={confirm} />
      ) : (
        <>
          <div className="everyport:flex everyport:min-w-0 everyport:flex-1 everyport:flex-col everyport:gap-px everyport:pr-2.5">
            <div className="everyport:flex everyport:min-w-0 everyport:items-center everyport:gap-1.5 everyport:overflow-clip">
              <span className="everyport:shrink-0 everyport:whitespace-pre everyport:text-13 everyport:font-medium everyport:text-fg">{server.project.name}</span>
              {server.project.branch && (
                <span className="everyport:flex everyport:min-w-0 everyport:items-center everyport:gap-1 everyport:text-fg2">
                  <BranchIcon />
                  <span className="everyport:clamp-1 everyport:text-13">{server.project.branch}</span>
                </span>
              )}
            </div>
            <div className={`everyport:flex everyport:min-w-0 everyport:items-center everyport:gap-[5px] everyport:text-11 ${warn ? "everyport:text-warn" : "everyport:text-fg2"}`}>
              {!note && !warn && icon}
              <span className="everyport:clamp-1">{note ?? context.text}</span>
            </div>
          </div>
          {active ? (
            // Pointer shortcuts; the keyboard has ⌘O and ⌘⌫, so they stay out of the listbox's tab order.
            <span className="everyport:flex everyport:shrink-0 everyport:items-center everyport:gap-0.5" aria-hidden>
              <button
                type="button"
                tabIndex={-1}
                aria-label="Open in browser"
                onClick={(event) => {
                  event.stopPropagation();
                  ctx.open(server);
                }}
                className="everyport:flex everyport:size-[22px] everyport:items-center everyport:justify-center everyport:rounded-md everyport:bg-accent everyport:text-fg everyport:hover:bg-[color-mix(in_oklab,var(--accent),var(--foreground)_8%)]"
              >
                <OpenIcon />
              </button>
              <button
                type="button"
                tabIndex={-1}
                aria-label={server.protected ? "Stop, protected" : "Stop"}
                title={server.protected ? "Protected. Stop asks first." : undefined}
                onClick={(event) => {
                  event.stopPropagation();
                  ctx.stop(server);
                }}
                className="everyport:relative everyport:flex everyport:size-[22px] everyport:items-center everyport:justify-center everyport:rounded-md everyport:text-fg/80 everyport:hover:bg-accent"
              >
                <StopIcon />
                {server.protected && <ProtectedBadge />}
              </button>
            </span>
          ) : (
            <Sparkline history={server.history} warn={attention} />
          )}
          <span className={`everyport:w-[58px] everyport:shrink-0 everyport:text-right everyport:font-mono everyport:text-13 ${attention ? "everyport:text-warn" : "everyport:text-fg/85"}`}>
            {memory(server.memory)}
          </span>
        </>
      )}
    </div>
  );
}

/** Ports of other users and the system: shown for reference, with no actions. */
function OtherPorts({ ports }: { ports: OtherPort[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="everyport:px-1.5 everyport:pb-1.5">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="everyport:flex everyport:w-full everyport:items-center everyport:gap-1 everyport:rounded-[9px] everyport:px-2.5 everyport:py-[7px] everyport:hover:bg-accent"
      >
        <span className="everyport:text-13 everyport:text-fg2">Other ports</span>
        <span className="everyport:font-mono everyport:text-13 everyport:text-fg3">· {ports.length}</span>
        <Chevron direction={open ? "down" : "right"} className="everyport:text-fg3" />
      </button>
      <div className="everyport-grow" data-closed={!open || undefined} inert={!open}>
        {/* `.everyport-grow > *` clips its child, so the scroller sits one level in. */}
        <div>
          <div className="everyport-scroll" style={{ maxHeight: VISIBLE_ROWS * 26 }}>
          {ports.map((other) => (
            <div
              key={other.port}
              aria-label={`Port ${other.port}${other.process_name ? `, ${other.process_name}` : ""}${other.owner ? `, owned by ${other.owner}` : ""}`}
              className="everyport:flex everyport:items-center everyport:px-2.5 everyport:py-[5px] everyport:text-fg2"
            >
              <span className="everyport:w-[9px] everyport:shrink-0" />
              <span className="selectable everyport:w-[46px] everyport:shrink-0 everyport:font-mono everyport:text-13">{other.port}</span>
              <span className="selectable everyport:clamp-1 everyport:flex-1 everyport:pr-2.5 everyport:text-13">{other.process_name}</span>
              <span className="everyport:shrink-0 everyport:text-11 everyport:text-fg3">{other.owner}</span>
            </div>
          ))}
          </div>
        </div>
      </div>
    </div>
  );
}
