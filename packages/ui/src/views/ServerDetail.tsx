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
  const restarting = ctx.pending.get(server.port) === "restarting";
  const canRestart = server.command !== null && server.cwd_exists;
  return (
    <div className="flex flex-col gap-2.5 px-4 pt-3 pb-4">
      <Header title={server.project.name} onBack={onBack} />
      <div className="flex items-center justify-between gap-1.5">
        <div className="flex items-center gap-1.5">
          <Colon status={server.status} color={ctx.colorOf(server.port)} large />
          <span className="selectable font-mono text-28 font-medium text-fg">{server.port}</span>
        </div>
        <div className="flex items-center gap-2.5">
          <span className="font-mono text-13 text-fg3">{restarting ? "restarting…" : up === null ? "" : `up ${duration(up)}`}</span>
          <div className="flex gap-1.5">
            <button
              type="button"
              aria-label="Restart"
              title={server.agent ? `Restart. It runs outside the ${agentName[server.agent.kind]} session.` : "Restart with the same command"}
              disabled={!canRestart || restarting}
              onClick={() => ctx.restart(server)}
              className="flex size-[26px] shrink-0 items-center justify-center rounded-[7px] bg-accent text-fg disabled:opacity-40"
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
              className="flex size-[26px] shrink-0 items-center justify-center rounded-[7px] bg-danger/14 text-danger"
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
    <div className="flex h-[30px] shrink-0 items-center gap-3" title={title}>
      <span className="w-16 shrink-0 text-11 text-fg3">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1.5">{value}</div>
    </div>
  );
}

const Value = ({ children, mono = false, strong = false }: { children: ReactNode; mono?: boolean; strong?: boolean }) => (
  <span className={`selectable clamp-1 text-13 ${mono ? "font-mono" : ""} ${strong ? "text-fg" : "text-fg2"}`}>{children}</span>
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
          className="flex min-w-0 flex-1 items-center gap-1.5"
        >
          <AgentIcon kind={agent.kind} className="text-fg2" />
          <span className="clamp-1 text-13 text-fg">{agent.title ?? agentName[agent.kind]}</span>
          <span className="shrink-0 whitespace-pre font-mono text-13 text-fg3">{agent.id.slice(0, 8)}</span>
          <span className="flex-1" />
          <OpenIcon className="text-fg2" />
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
        <button type="button" onClick={() => client.openWorkspace?.(machineId, server.port)} className="flex min-w-0 flex-1 items-center gap-1.5">
          <Value>{text}</Value>
          <span className="flex-1" />
          <OpenIcon className="text-fg2" />
        </button>
      ) : (
        <Value>{text}</Value>
      ),
    });
  }
  if ((agent || project.branch) && folder) secondary.push({ label: "Folder", title: server.cwd ?? undefined, value: <Value mono>{folder}</Value> });
  if (project.framework) secondary.push({ label: "Framework", value: <Value>{project.framework}</Value> });
  if (server.command) secondary.push({ label: "Command", title: server.command, value: <Value mono>{server.command}</Value> });
  if (server.started_at !== null) secondary.push({ label: "Started", value: <Value mono>{started(server.started_at, ctx.now)}</Value> });
  if (server.addresses.length) secondary.push({ label: "Address", value: <Value mono>{server.addresses.join(" · ")}</Value> });

  const workspaceFolder = project.root ?? server.cwd;
  const menu: MenuItem[] = [];
  if (agent) {
    if (workspace?.open_url && client.openWorkspace) {
      menu.push({ label: `Open in ${workspaceApp[workspace.kind]}`, hint: workspace.name, onSelect: () => client.openWorkspace?.(machineId, server.port) });
    } else if (workspace && workspaceFolder && client.revealFolder) {
      menu.push({ label: `Reveal ${workspace.name} workspace`, onSelect: () => client.revealFolder?.(machineId, workspaceFolder) });
    }
    if (client.resumeSession) {
      menu.push({ label: "Resume in Terminal", hint: commandHint(agent.resume_command), onSelect: () => client.resumeSession?.(machineId, agent) });
    }
    const transcript = agent.transcript_path;
    if (transcript && client.revealFolder) menu.push({ label: "Show transcript", onSelect: () => client.revealFolder?.(machineId, transcript) });
    if (menu.length) menu.push("divider");
    menu.push({ label: "Copy session ID", hint: agent.id.slice(0, 8), onSelect: () => copy(agent.id) });
  }

  return (
    <div className="hairline-t relative flex flex-col px-4 py-1.5">
      {primary.map((row) => (
        <InfoRow key={row.label} {...row} />
      ))}
      {secondary.length > 0 && (
        <>
          <div className="ppm-grow" data-closed={!expanded || undefined} inert={!expanded}>
            <div className="flex flex-col">
              {secondary.map((row) => (
                <InfoRow key={row.label} {...row} />
              ))}
            </div>
          </div>
          <InfoRow
            label=""
            value={
              <button type="button" aria-expanded={expanded} onClick={() => setExpanded(!expanded)} className="flex items-center gap-1 text-11 text-fg3">
                {expanded ? "Less" : `${secondary.length} more`}
                <Chevron direction={expanded ? "up" : "down"} className="text-fg3" />
              </button>
            }
          />
        </>
      )}
      {agent && sessionMenu && (
        <Menu
          className="top-[42px] left-[84px] w-[300px]"
          onClose={() => setSessionMenu(false)}
          title={
            <>
              <AgentIcon kind={agent.kind} className="text-fg2" />
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
      <div className="hairline-t flex flex-col gap-3.5 px-4 pt-3 pb-4">
        <ChartHead title="Memory" value={memory(hovered?.memory ?? server.memory)} note={<span className="font-mono">{time ?? "10 min"}</span>} />
        <MemoryChart history={server.history} now={ctx.now} hover={hover} onHover={onHover} threshold={ctx.alertMemory} />
      </div>
      <div className="flex flex-col gap-3.5 px-4 pb-4">
        <ChartHead
          title="CPU"
          value={percent(hovered?.cpu_percent ?? server.cpu_percent)}
          note={time ? <span className="font-mono">{time}</span> : server.history.length ? `peak ${percent(peak)}` : null}
        />
        <CpuChart history={server.history} now={ctx.now} hover={hover} onHover={onHover} />
      </div>
    </>
  );
}

function ChartHead({ title, value, note }: { title: string; value: string; note: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between">
      <div className="flex items-baseline gap-2">
        <span className="text-13 font-medium text-fg2">{title}</span>
        <span className="font-mono text-13 font-medium text-fg">{value}</span>
      </div>
      <span className="text-11 text-fg3">{note}</span>
    </div>
  );
}

function Processes({ server }: { server: Server }) {
  const [open, setOpen] = useState(false);
  const largest = Math.max(1, ...server.processes.map((process) => process.memory));
  return (
    <div className={`hairline-t flex flex-col gap-2 px-4 ${open ? "pt-3 pb-4" : "py-3.5"}`}>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex h-4 shrink-0 items-center justify-between">
        <span className="flex items-center gap-1">
          <span className="text-13 text-fg2">Processes</span>
          <Chevron direction={open ? "down" : "right"} className="text-fg3" />
        </span>
        <span className="flex items-center gap-2 font-mono text-13">
          <span className="text-fg3">{server.processes.length} ·</span>
          <span className="text-fg2">{memory(server.memory)}</span>
        </span>
      </button>
      {open && (
        <div className="flex flex-col gap-[5px]">
          {server.processes.map((process) => {
            const main = process.proc.pid === server.pid;
            const indent = process.depth > 0 ? `${"  ".repeat(process.depth - 1)}└ ` : "";
            return (
              <div key={process.proc.pid} className="flex items-center" title={process.name}>
                <span className={`selectable clamp-1 flex-1 whitespace-pre font-mono text-13 ${main ? "text-fg" : "text-fg/80"}`}>
                  {indent}
                  {process.name}
                </span>
                <span className="selectable w-16 shrink-0 font-mono text-11 text-fg3">{process.proc.pid}</span>
                <span className="flex h-1 w-16 shrink-0 rounded-sm bg-accent">
                  <span
                    className={`h-1 rounded-sm ${main ? "bg-fg/75" : "bg-fg/45"}`}
                    style={{ width: Math.max(3, (64 * process.memory) / largest) }}
                  />
                </span>
                <span className={`w-16 shrink-0 text-right font-mono text-13 ${main ? "text-fg/90" : "text-fg/70"}`}>{memory(process.memory)}</span>
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
    if (client.openInEditor) items.push({ label: "Open in editor", onSelect: () => client.openInEditor?.(machineId, folder) });
    if (client.revealFolder) items.push({ label: "Reveal folder", onSelect: () => client.revealFolder?.(machineId, folder) });
  }
  items.push("divider", { label: "Force stop", danger: true, onSelect: () => ctx.stop(server, true) });

  return (
    <div className="hairline-t relative flex items-center gap-2 p-3">
      <button
        type="button"
        onClick={() => ctx.open(server)}
        className="flex flex-1 items-center justify-center rounded-lg bg-primary px-3 py-[7px] text-13 font-medium text-on-primary hover:opacity-90"
      >
        Open localhost:{server.port}
      </button>
      {preview && client.openExternal && (
        <button
          type="button"
          title={preview}
          onClick={() => client.openExternal?.(preview)}
          className="flex items-center gap-1.5 rounded-lg bg-accent px-3 py-[7px] text-13 text-fg"
        >
          <VercelIcon className="text-fg2" />
          Preview
        </button>
      )}
      <button
        type="button"
        aria-label="More"
        aria-haspopup="menu"
        aria-expanded={more}
        onClick={() => setMore(true)}
        className="flex size-[30px] shrink-0 items-center justify-center gap-[3px] rounded-lg bg-accent"
      >
        <span className="size-[3px] rounded-full bg-fg" />
        <span className="size-[3px] rounded-full bg-fg" />
        <span className="size-[3px] rounded-full bg-fg" />
      </button>
      {more && <Menu className="right-3 bottom-[50px] w-[200px]" onClose={() => setMore(false)} items={items} />}
    </div>
  );
}
