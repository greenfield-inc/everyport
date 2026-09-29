// What each view says about a server, derived from protocol data. Times are
// relative to the snapshot's `taken_at`, so a snapshot always reads the same.
import type { CleanUpReason, Server, WorkspaceKind } from "@ppm/protocol";
import { duration, parentFolder, shortDuration, total } from "./format.ts";

/** Default `Config.alert_memory`: 2 GB. */
export const DEFAULT_ALERT_MEMORY = 2 * 1024 ** 3;

const HOUR = 3600;

export const uptime = (server: Server, now: number) =>
  server.started_at === null ? null : Math.max(0, (now - server.started_at) / 1000);

export const idleFor = (server: Server, now: number) => Math.max(0, (now - server.last_active) / 1000);

/** Seconds between the first and last history sample. */
export function historySpan(server: Server): number {
  const { history } = server;
  return history.length < 2 ? 0 : (history[history.length - 1].at - history[0].at) / 1000;
}

/** Memory growth over the history window. */
export function growth(server: Server): number {
  if (server.clean_up?.kind === "leaking") return server.clean_up.bytes;
  const { history } = server;
  return history.length < 2 ? 0 : history[history.length - 1].memory - history[0].memory;
}

export const isLeaking = (server: Server) => server.clean_up?.kind === "leaking";

/** A deleted worktree dims its row. */
export const isGone = (server: Server) => server.status === "idle" && !server.cwd_exists;

/** "+1.1 GB in 10 min" */
export function growthText(server: Server): string {
  const span = historySpan(server);
  const window = span < HOUR ? `${Math.max(1, Math.round(span / 60))} min` : shortDuration(span);
  return `+${total(growth(server))} in ${window}`;
}

/** Where the server runs, when there is no better name: its workspace or parent folder. */
function location(server: Server): string | null {
  if (server.workspace) return server.workspace.name;
  const folder = server.project.root ?? server.cwd;
  return folder ? parentFolder(folder) : null;
}

export type RowContext = { text: string; warn: boolean };

/** The second line of a server row. */
export function rowContext(server: Server, now: number, alertMemory: number): RowContext {
  if (server.status === "attention") {
    const text = isLeaking(server) ? growthText(server) : `Over ${total(alertMemory)}`;
    return { text, warn: true };
  }
  const idle = idleFor(server, now);
  if (!server.cwd_exists) return { text: `Worktree deleted · idle ${shortDuration(idle)}`, warn: false };
  const up = uptime(server, now);
  const time = idle > HOUR ? `idle ${shortDuration(idle)}` : up === null ? null : `up ${shortDuration(up)}`;
  return { text: [location(server), time].filter(Boolean).join(" · "), warn: false };
}

/** Why Clean up suggests this server. */
export function reasonText(reason: CleanUpReason, server: Server, now: number): string {
  switch (reason.kind) {
    case "worktree_deleted":
      return `Worktree deleted · idle ${shortDuration(idleFor(server, now))}`;
    case "idle":
      return server.connections === 0
        ? `Idle ${shortDuration(reason.seconds)} · no connections`
        : `Idle ${shortDuration(reason.seconds)}`;
    case "long_running":
      return `Running for ${duration(reason.seconds)}`;
    case "leaking":
      return `Leaking · ${growthText(server)}`;
  }
}

/** Clean up preselects everything it suggests except leaking servers, which are usually still in use. */
export const preselected = (server: Server) => server.clean_up !== null && server.clean_up.kind !== "leaking";

export const workspaceApp: Record<WorkspaceKind, string> = {
  conductor: "Conductor",
  pane: "Pane",
  git_worktree: "Git worktree",
};

export const agentName = { claude_code: "Claude Code", codex: "Codex" } as const;

/** "claude --resume <id>" shows as "claude --resume". */
export const commandHint = (command: string) => command.split(/\s+/).slice(0, 2).join(" ");

/** "postgres :5432 and redis-server :6379 are protected" */
export function protectedNote(servers: Server[]): string | null {
  if (!servers.length) return null;
  const names = servers.map((server) => `${server.process_name} :${server.port}`);
  const list = names.length === 1 ? names[0] : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  return `${list} ${names.length === 1 ? "is" : "are"} protected`;
}
