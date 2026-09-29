import type { Server } from "@ppm/protocol";
import { useEffect, useRef, useState } from "react";
import { Sparkline } from "../charts.tsx";
import { Colon, Grow, Header, useLeaving } from "../components.tsx";
import type { Pending, ViewContext } from "../context.ts";
import { memory, memoryParts, percent, total, totalParts } from "../format.ts";
import { AgentIcon, BranchIcon, BroomIcon, DotGrid, GearIcon, OpenIcon, StopIcon, WorkspaceIcon } from "../icons.tsx";
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
      <div className="hairline-t">
        {servers.length ? (
          <Rows ctx={ctx} selected={selected} segment={segment} onHover={setHoveredRow} onOpen={onOpen} />
        ) : (
          <div className="flex flex-col items-center gap-3 py-7 text-center">
            <span className="text-fg3">
              <DotGrid size={48} />
            </span>
            <span className="text-13 text-fg2">Nothing listening</span>
            <span className="text-11 text-fg3">Dev servers show up here when they start.</span>
          </div>
        )}
      </div>
      <div className="hairline-t flex items-center justify-between p-3">
        <button
          type="button"
          onClick={onCleanUp}
          disabled={!servers.length}
          className="flex items-center gap-[7px] rounded-[7px] bg-accent py-[5px] pr-2.5 pl-2 shadow-[inset_0_0.5px_0_color-mix(in_oklab,var(--foreground)_12%,transparent)] disabled:opacity-50"
        >
          <BroomIcon className="text-fg" />
          <span className="text-13 font-medium text-fg">Clean up</span>
          {suggested > 0 && <span className="font-mono text-11 font-medium text-fg2">{suggested}</span>}
        </button>
        {client.openSettings && (
          <button
            type="button"
            aria-label="Settings"
            onClick={() => client.openSettings?.()}
            className="-m-[5px] flex size-[26px] items-center justify-center rounded-[7px] text-fg2 hover:bg-accent"
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
    <div className="flex flex-col gap-2.5 px-4 pt-3 pb-4">
      <Header title={focused ? `${focused.project.name} :${focused.port}` : "Servers"} />
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-1.5">
          <div
            className="flex items-center gap-1.5"
            onPointerEnter={() => setShowMemory(true)}
            onPointerLeave={() => setShowMemory(false)}
          >
            <span className="font-mono text-28 font-medium text-fg">{amount}</span>
            <span className="self-end pb-1 font-mono text-13 text-fg3">{unit}</span>
          </div>
          <span className="ml-auto grow text-right font-mono text-13 text-fg3">{detail}</span>
        </div>
        <div className="flex h-4 shrink-0 items-center" onPointerLeave={() => props.onSegment(null)}>
          <div className="flex h-2 w-full items-center gap-0.5" aria-label="Memory by server" role="group">
            {servers.map((server, index) => (
              <button
                key={server.port}
                type="button"
                tabIndex={-1}
                aria-label={`${server.project.name} :${server.port} · ${memory(server.memory)}`}
                onPointerEnter={() => props.onSegment(server.port)}
                onClick={() => props.onOpen(server)}
                className="h-full min-w-0.5 transition-[opacity] duration-100"
                style={{ flex: `${server.memory} 0 0`, opacity: lit !== null && lit !== server.port ? 0.35 : 1 }}
              >
                <span
                  className="block w-full transition-[height] duration-100"
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
      aria-activedescendant={selected === null ? undefined : `ppm-server-${selected}`}
      data-live={live || undefined}
      className="ppm-scroll p-1.5"
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
      className={`flex items-center rounded-[9px] px-2.5 py-[9px] transition-opacity duration-100 ${active ? "bg-accent" : ""}`}
      style={{ opacity }}
    >
      <Colon status={server.status} color={ctx.colorOf(server.port)} />
      <span className="w-[46px] shrink-0 font-mono text-13 font-medium text-fg">{server.port}</span>
      <div className="flex min-w-0 flex-1 flex-col gap-px pr-2.5">
        <div className="flex min-w-0 items-center gap-1.5 overflow-clip">
          <span className="shrink-0 whitespace-pre text-13 font-medium text-fg">{server.project.name}</span>
          {server.project.branch && (
            <span className="flex min-w-0 items-center gap-1 text-fg2">
              <BranchIcon />
              <span className="clamp-1 text-13">{server.project.branch}</span>
            </span>
          )}
        </div>
        <div className={`flex min-w-0 items-center gap-[5px] text-11 ${warn ? "text-warn" : "text-fg2"}`}>
          {!note && !warn && icon}
          <span className="clamp-1">{note ?? context.text}</span>
        </div>
      </div>
      {active ? (
        <span className="flex shrink-0 items-center gap-0.5">
          <button
            type="button"
            aria-label="Open in browser"
            onClick={(event) => {
              event.stopPropagation();
              ctx.open(server);
            }}
            className="flex size-[22px] items-center justify-center rounded-md bg-accent text-fg hover:bg-[color-mix(in_oklab,var(--accent),var(--foreground)_8%)]"
          >
            <OpenIcon />
          </button>
          <button
            type="button"
            aria-label="Stop"
            onClick={(event) => {
              event.stopPropagation();
              ctx.stop(server);
            }}
            className="flex size-[22px] items-center justify-center rounded-md text-fg/80 hover:bg-accent"
          >
            <StopIcon />
          </button>
        </span>
      ) : (
        <Sparkline history={server.history} warn={attention} />
      )}
      <span className={`w-[58px] shrink-0 text-right font-mono text-13 ${attention ? "text-warn" : "text-fg/85"}`}>
        {memory(server.memory)}
      </span>
    </div>
  );
}
