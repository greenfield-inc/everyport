import type { Server } from "@ppm/protocol";
import { type ReactNode, useState } from "react";
import { CpuChart, MemoryChart, sampleNear } from "../charts.tsx";
import { Colon, Header, Menu, type MenuItem } from "../components.tsx";
import { copy, type ViewContext } from "../context.ts";
import { clock, clockSeconds, duration, memory, percent, shortPath, started } from "../format.ts";
import { AgentIcon, Chevron, OpenIcon, RestartIcon, VercelIcon } from "../icons.tsx";
import { agentName, commandHint, uptime, workspaceApp } from "../model.ts";

type Props = {
  ctx: ViewContext;
  server: Server;
  onBack: () => void;
};

export function ServerDetail({ ctx, server, onBack }: Props) {
  const [hover, setHover] = useState<number | null>(null);
  return (
    <>
      <DetailHeader ctx={ctx} server={server} onBack={onBack} />
      <Info ctx={ctx} server={server} />
      <Charts ctx={ctx} server={server} hover={hover} onHover={setHover} />
      <Processes server={server} />
      <Footer ctx={ctx} server={server} />
    </>
  );
}

function DetailHeader({ ctx, server, onBack }: Props) {
  const up = uptime(server, ctx.now);
  const pending = ctx.pending.get(server.port);
  const restarting = pending === "restarting";
  const error = typeof pending === "object" ? pending.error : null;
  const canRestart = server.command !== null && server.cwd_exists;
  return (
    <div className="ppm:flex ppm:flex-col ppm:gap-2.5 ppm:px-4 ppm:pt-3 ppm:pb-4">
      <Header title={server.project.name} onBack={onBack} />
      <div className="ppm:flex ppm:items-center ppm:justify-between ppm:gap-1.5">
        <div className="ppm:flex ppm:items-center ppm:gap-1.5">
          <Colon status={server.status} color={ctx.colorOf(server.port)} large />
          <span className="selectable ppm:font-mono ppm:text-28 ppm:font-medium ppm:text-fg">{server.port}</span>
        </div>
        <div className="ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-2.5">
          {error ? (
            <span className="ppm:clamp-1 ppm:text-11 ppm:text-danger" title={error}>
              {error}
            </span>
          ) : (
            <span className="ppm:font-mono ppm:text-13 ppm:text-fg3">{restarting ? "restarting…" : up === null ? "" : `up ${duration(up)}`}</span>
          )}
          <div className="ppm:flex ppm:gap-1.5">
            <button
              type="button"
              aria-label="Restart"
              title={server.agent ? `Restart. It runs outside the ${agentName[server.agent.kind]} session.` : "Restart with the same command"}
              disabled={!canRestart || restarting}
              onClick={() => ctx.restart(server)}
              className="ppm:flex ppm:size-[26px] ppm:shrink-0 ppm:items-center ppm:justify-center ppm:rounded-[7px] ppm:bg-accent ppm:text-fg ppm:disabled:opacity-40"
            >
              <RestartIcon className={restarting ? "ppm-spin" : undefined} />
            </button>
            <button
              type="button"
              aria-label="Stop"
              title={`Stop ${server.processes.length} ${server.processes.length === 1 ? "process" : "processes"}`}
              onClick={() => {
                ctx.stop(server);
                onBack();
              }}
              className="ppm:flex ppm:size-[26px] ppm:shrink-0 ppm:items-center ppm:justify-center ppm:rounded-[7px] ppm:bg-danger/14 ppm:text-danger"
            >
              <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden>
                <rect x="2" y="2" width="8" height="8" rx="1.8" fill="currentColor" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

type Row = { label: string; value: ReactNode; title?: string };

function InfoRow({ label, value, title }: Row) {
  return (
    <div className="ppm:flex ppm:h-[30px] ppm:shrink-0 ppm:items-center ppm:gap-3" title={title}>
      <span className="ppm:w-16 ppm:shrink-0 ppm:text-11 ppm:text-fg3">{label}</span>
      <div className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:items-center ppm:gap-1.5">{value}</div>
    </div>
  );
}

const Value = ({ children, mono = false, strong = false }: { children: ReactNode; mono?: boolean; strong?: boolean }) => (
  <span className={`selectable ppm:clamp-1 ppm:text-13 ${mono ? "ppm:font-mono" : ""} ${strong ? "ppm:text-fg" : "ppm:text-fg2"}`}>{children}</span>
);

function Info({ ctx, server }: { ctx: ViewContext; server: Server }) {
  const [expanded, setExpanded] = useState(false);
  const [sessionMenu, setSessionMenu] = useState(false);
  const { agent, workspace, project } = server;
  const { client, machineId } = ctx;
  const folder = server.cwd ? shortPath(server.cwd) : null;

  const primary: Row[] = [];
  if (agent) {
    primary.push({
      label: "Session",
      title: agent.title ?? undefined,
      value: (
        <button
          type="button"
          aria-haspopup="menu"
          aria-expanded={sessionMenu}
          onClick={() => setSessionMenu(true)}
          className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:items-center ppm:gap-1.5"
        >
          <AgentIcon kind={agent.kind} className="ppm:text-fg2" />
          <span className="ppm:clamp-1 ppm:text-13 ppm:text-fg">{agent.title ?? agentName[agent.kind]}</span>
          <span className="ppm:shrink-0 ppm:whitespace-pre ppm:font-mono ppm:text-13 ppm:text-fg3">{agent.id.slice(0, 8)}</span>
          <span className="ppm:flex-1" />
          <OpenIcon className="ppm:text-fg2" />
        </button>
      ),
    });
  }
  if (project.branch) primary.push({ label: "Branch", title: project.branch, value: <Value strong>{project.branch}</Value> });
  if (primary.length < 2 && folder) primary.push({ label: "Folder", title: server.cwd ?? undefined, value: <Value mono>{folder}</Value> });

  const secondary: Row[] = [];
  if (workspace) {
    const text = `${workspaceApp[workspace.kind]} · ${workspace.name}`;
    const openable = workspace.open_url !== null && client.openWorkspace;
    secondary.push({
      label: "Workspace",
      title: text,
      value: openable ? (
        <button type="button" onClick={() => ctx.act(server, () => client.openWorkspace?.(machineId, server.port))} className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:items-center ppm:gap-1.5">
          <Value>{text}</Value>
          <span className="ppm:flex-1" />
          <OpenIcon className="ppm:text-fg2" />
        </button>
      ) : (
        <Value>{text}</Value>
      ),
    });
  }
  if (folder && !primary.some((row) => row.label === "Folder")) secondary.push({ label: "Folder", title: server.cwd ?? undefined, value: <Value mono>{folder}</Value> });
  if (project.framework) secondary.push({ label: "Framework", value: <Value>{project.framework}</Value> });
  if (server.command) secondary.push({ label: "Command", title: server.command, value: <Value mono>{server.command}</Value> });
  if (server.started_at !== null) secondary.push({ label: "Started", value: <Value mono>{started(server.started_at, ctx.now)}</Value> });
  if (server.addresses.length) secondary.push({ label: "Address", value: <Value mono>{server.addresses.join(" · ")}</Value> });

  const workspaceFolder = project.root ?? server.cwd;
  const menu: MenuItem[] = [];
  if (agent) {
    if (workspace?.open_url && client.openWorkspace) {
      menu.push({ label: `Open in ${workspaceApp[workspace.kind]}`, hint: workspace.name, onSelect: () => ctx.act(server, () => client.openWorkspace?.(machineId, server.port)) });
    } else if (workspace && workspaceFolder && client.revealFolder) {
      menu.push({ label: `Reveal ${workspace.name} workspace`, onSelect: () => ctx.act(server, () => client.revealFolder?.(machineId, workspaceFolder)) });
    }
    if (client.resumeSession) {
      menu.push({ label: "Resume in Terminal", hint: commandHint(agent.resume_command), onSelect: () => ctx.act(server, () => client.resumeSession?.(machineId, agent)) });
    }
    const transcript = agent.transcript_path;
    if (transcript && client.revealFolder) menu.push({ label: "Show transcript", onSelect: () => ctx.act(server, () => client.revealFolder?.(machineId, transcript)) });
    if (menu.length) menu.push("divider");
    menu.push({ label: "Copy session ID", hint: agent.id.slice(0, 8), onSelect: () => copy(agent.id) });
  }

  return (
    <div className="ppm:hairline-t ppm:relative ppm:flex ppm:flex-col ppm:px-4 ppm:py-1.5">
      {primary.map((row) => (
        <InfoRow key={row.label} {...row} />
      ))}
      {secondary.length > 0 && (
        <>
          <div className="ppm-grow" data-closed={!expanded || undefined} inert={!expanded}>
            <div className="ppm:flex ppm:flex-col">
              {secondary.map((row) => (
                <InfoRow key={row.label} {...row} />
              ))}
            </div>
          </div>
          <InfoRow
            label=""
            value={
              <button type="button" aria-expanded={expanded} onClick={() => setExpanded(!expanded)} className="ppm:flex ppm:items-center ppm:gap-1 ppm:text-11 ppm:text-fg3">
                {expanded ? "Less" : `${secondary.length} more`}
                <Chevron direction={expanded ? "up" : "down"} className="ppm:text-fg3" />
              </button>
            }
          />
        </>
      )}
      {agent && sessionMenu && (
        <Menu
          className="ppm:top-[42px] ppm:left-[84px] ppm:w-[300px]"
          onClose={() => setSessionMenu(false)}
          title={
            <>
              <AgentIcon kind={agent.kind} className="ppm:text-fg2" />
              {agentName[agent.kind]}
              {agent.started_at !== null && ` · started ${clock(agent.started_at)}`}
            </>
          }
          items={menu}
        />
      )}
    </div>
  );
}

function Charts({ ctx, server, hover, onHover }: { ctx: ViewContext; server: Server; hover: number | null; onHover: (time: number | null) => void }) {
  const hovered = sampleNear(server.history, hover);
  const peak = Math.max(0, ...server.history.map((sample) => sample.cpu_percent));
  const time = hovered ? clockSeconds(hovered.at) : null;
  return (
    <>
      <div className="ppm:hairline-t ppm:flex ppm:flex-col ppm:gap-3.5 ppm:px-4 ppm:pt-3 ppm:pb-4">
        <ChartHead title="Memory" value={memory(hovered?.memory ?? server.memory)} note={<span className="ppm:font-mono">{time ?? "10 min"}</span>} />
        <MemoryChart history={server.history} now={ctx.now} hover={hover} onHover={onHover} threshold={ctx.alertMemory} />
      </div>
      <div className="ppm:flex ppm:flex-col ppm:gap-3.5 ppm:px-4 ppm:pb-4">
        <ChartHead
          title="CPU"
          value={percent(hovered?.cpu_percent ?? server.cpu_percent)}
          note={time ? <span className="ppm:font-mono">{time}</span> : server.history.length ? `peak ${percent(peak)}` : null}
        />
        <CpuChart history={server.history} now={ctx.now} hover={hover} onHover={onHover} />
      </div>
    </>
  );
}

function ChartHead({ title, value, note }: { title: string; value: string; note: ReactNode }) {
  return (
    <div className="ppm:flex ppm:items-baseline ppm:justify-between">
      <div className="ppm:flex ppm:items-baseline ppm:gap-2">
        <span className="ppm:text-13 ppm:font-medium ppm:text-fg2">{title}</span>
        <span className="ppm:font-mono ppm:text-13 ppm:font-medium ppm:text-fg">{value}</span>
      </div>
      <span className="ppm:text-11 ppm:text-fg3">{note}</span>
    </div>
  );
}

function Processes({ server }: { server: Server }) {
  const [open, setOpen] = useState(false);
  const largest = Math.max(1, ...server.processes.map((process) => process.memory));
  return (
    <div className={`ppm:hairline-t ppm:flex ppm:flex-col ppm:gap-2 ppm:px-4 ${open ? "ppm:pt-3 ppm:pb-4" : "ppm:py-3.5"}`}>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="ppm:flex ppm:h-4 ppm:shrink-0 ppm:items-center ppm:justify-between">
        <span className="ppm:flex ppm:items-center ppm:gap-1">
          <span className="ppm:text-13 ppm:text-fg2">Processes</span>
          <Chevron direction={open ? "down" : "right"} className="ppm:text-fg3" />
        </span>
        <span className="ppm:flex ppm:items-center ppm:gap-2 ppm:font-mono ppm:text-13">
          <span className="ppm:text-fg3">{server.processes.length} ·</span>
          <span className="ppm:text-fg2">{memory(server.memory)}</span>
        </span>
      </button>
      {open && (
        <div className="ppm:flex ppm:flex-col ppm:gap-[5px]">
          {server.processes.map((process) => {
            const main = process.proc.pid === server.pid;
            const indent = process.depth > 0 ? `${"  ".repeat(process.depth - 1)}└ ` : "";
            return (
              <div key={process.proc.pid} className="ppm:flex ppm:items-center" title={process.name}>
                <span className={`selectable ppm:clamp-1 ppm:flex-1 ppm:whitespace-pre ppm:font-mono ppm:text-13 ${main ? "ppm:text-fg" : "ppm:text-fg/80"}`}>
                  {indent}
                  {process.name}
                </span>
                <span className="selectable ppm:w-16 ppm:shrink-0 ppm:font-mono ppm:text-11 ppm:text-fg3">{process.proc.pid}</span>
                <span className="ppm:flex ppm:h-1 ppm:w-16 ppm:shrink-0 ppm:rounded-sm ppm:bg-accent">
                  <span
                    className={`ppm:h-1 ppm:rounded-sm ${main ? "ppm:bg-fg/75" : "ppm:bg-fg/45"}`}
                    style={{ width: Math.max(3, (64 * process.memory) / largest) }}
                  />
                </span>
                <span className={`ppm:w-16 ppm:shrink-0 ppm:text-right ppm:font-mono ppm:text-13 ${main ? "ppm:text-fg/90" : "ppm:text-fg/70"}`}>{memory(process.memory)}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Footer({ ctx, server }: { ctx: ViewContext; server: Server }) {
  const [more, setMore] = useState(false);
  const { client, machineId } = ctx;
  const preview = server.project.vercel?.preview_url;
  const folder = server.project.root ?? server.cwd;
  const items: MenuItem[] = [{ label: "Copy URL", onSelect: () => copy(`http://localhost:${server.port}`) }];
  if (server.command) items.push({ label: "Copy command", onSelect: () => copy(server.command ?? "") });
  if (folder && server.cwd_exists && (client.openInEditor || client.revealFolder)) {
    items.push("divider");
    if (client.openInEditor) items.push({ label: "Open in editor", onSelect: () => ctx.act(server, () => client.openInEditor?.(machineId, folder)) });
    if (client.revealFolder) items.push({ label: "Reveal folder", onSelect: () => ctx.act(server, () => client.revealFolder?.(machineId, folder)) });
  }
  items.push("divider", { label: "Force stop", danger: true, onSelect: () => ctx.stop(server, true) });

  return (
    <div className="ppm:hairline-t ppm:relative ppm:flex ppm:items-center ppm:gap-2 ppm:p-3">
      <button
        type="button"
        onClick={() => ctx.open(server)}
        className="ppm:flex ppm:flex-1 ppm:items-center ppm:justify-center ppm:rounded-lg ppm:bg-primary ppm:px-3 ppm:py-[7px] ppm:text-13 ppm:font-medium ppm:text-on-primary ppm:hover:opacity-90"
      >
        Open localhost:{server.port}
      </button>
      {preview && client.openExternal && (
        <button
          type="button"
          title={preview}
          onClick={() => ctx.act(server, () => client.openExternal?.(preview))}
          className="ppm:flex ppm:items-center ppm:gap-1.5 ppm:rounded-lg ppm:bg-accent ppm:px-3 ppm:py-[7px] ppm:text-13 ppm:text-fg"
        >
          <VercelIcon className="ppm:text-fg2" />
          Preview
        </button>
      )}
      <button
        type="button"
        aria-label="More"
        aria-haspopup="menu"
        aria-expanded={more}
        onClick={() => setMore(true)}
        className="ppm:flex ppm:size-[30px] ppm:shrink-0 ppm:items-center ppm:justify-center ppm:gap-[3px] ppm:rounded-lg ppm:bg-accent"
      >
        <span className="ppm:size-[3px] ppm:rounded-full ppm:bg-fg" />
        <span className="ppm:size-[3px] ppm:rounded-full ppm:bg-fg" />
        <span className="ppm:size-[3px] ppm:rounded-full ppm:bg-fg" />
      </button>
      {more && <Menu className="ppm:right-3 ppm:bottom-[50px] ppm:w-[200px]" onClose={() => setMore(false)} items={items} />}
    </div>
  );
}
