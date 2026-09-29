// The demo's machines: this computer runs the protocol fixture, and each
// remote machine runs its own believable servers.
import { fixtureSnapshot, type Machine, type Os, type Server, type Snapshot } from "@ppm/protocol";

const MB = 1024 * 1024;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

const [nextApp, , , , api] = fixtureSnapshot.servers;

type Spec = {
  port: number;
  name: string;
  framework: string | null;
  branch: string | null;
  cwd: string;
  command: string;
  /** Process tree, root first: name, MB, CPU %. */
  processes: [string, number, number][];
  up: number;
  /** Idle this long, so Clean up offers it. */
  idle?: number;
  protected?: boolean;
  agent?: Server["agent"];
};

/** A running server shaped like the fixture's first one, with its own name, tree and size. */
function server(spec: Spec, at: number): Server {
  const base = spec.idle ? api : nextApp;
  const memory = spec.processes.reduce((sum, [, mb]) => sum + mb * MB, 0);
  const cpu = spec.processes.reduce((sum, [, , cpu]) => sum + cpu, 0);
  const started_at = at - spec.up;
  const pid = 20000 + spec.port;
  return {
    ...base,
    port: spec.port,
    pid: pid + 1,
    root: { pid, started_at },
    process_name: spec.processes.at(-1)![0].split(/[ :]/)[0],
    addresses: ["0.0.0.0"],
    cwd: spec.cwd,
    cwd_exists: true,
    launch_dir: spec.cwd,
    command: spec.command,
    started_at,
    project: { name: spec.name, root: spec.cwd, framework: spec.framework, branch: spec.branch, worktree: null, github: null, vercel: null },
    workspace: null,
    agent: spec.agent ?? null,
    protected: spec.protected ?? false,
    clean_up: spec.idle ? { kind: "idle", seconds: spec.idle / 1000 } : null,
    status: spec.idle ? "idle" : "running",
    last_active: spec.idle ? at - spec.idle : at,
    processes: spec.processes.map(([name, mb, cpu], depth) => ({
      proc: { pid: pid + depth, started_at: started_at + depth * 500 },
      name,
      depth,
      memory: mb * MB,
      cpu_percent: cpu,
    })),
    memory,
    cpu_percent: cpu,
    history: base.history.map((sample) => ({ ...sample, memory: (sample.memory * memory) / base.memory })),
  };
}

const postgres = (at: number, cwd: string) =>
  server(
    {
      port: 5432,
      name: "postgres",
      framework: null,
      branch: null,
      cwd,
      command: "postgres -D /var/lib/postgresql/16/main",
      processes: [["postgres", 38, 0.1], ["postgres: checkpointer", 12, 0], ["postgres: walwriter", 9, 0]],
      up: 9 * 24 * HOUR,
      protected: true,
    },
    at,
  );

/** Moves every timestamp in the fixture so it was taken `at`, keeping "up 3h" true. */
function shifted(snapshot: Snapshot, at: number): Snapshot {
  const delta = at - snapshot.taken_at;
  const move = (value: number | null) => (value === null ? null : value + delta);
  return {
    ...snapshot,
    taken_at: at,
    servers: snapshot.servers.map((s) => ({
      ...s,
      started_at: move(s.started_at),
      last_active: s.last_active + delta,
      root: { ...s.root, started_at: s.root.started_at + delta },
      agent: s.agent && { ...s.agent, started_at: move(s.agent.started_at) },
      processes: s.processes.map((p) => ({ ...p, proc: { ...p.proc, started_at: p.proc.started_at + delta } })),
      history: s.history.map((sample) => ({ ...sample, at: sample.at + delta })),
    })),
  };
}

/** The fixture's macOS paths, written the way `os` writes them. */
function localPaths(snapshot: Snapshot, os: Os): Snapshot {
  if (os === "macos") return snapshot;
  const json = JSON.stringify(snapshot).replace(/\/Users\/dev((?:\/[^"/]+)*)/g, (_, rest: string) =>
    os === "linux" ? `/home/dev${rest}` : `C:\\\\Users\\\\dev${rest.replaceAll("/", "\\\\")}`,
  );
  return JSON.parse(json) as Snapshot;
}

const host = (hostname: string, os: Os, arch = "x86_64", cores = 8) => ({ hostname, os, arch, cores });

export const LOCAL = "local";

const POSTGRES_DIR: Record<Os, string> = {
  macos: "/opt/homebrew/var/postgresql@16",
  windows: "C:\\Program Files\\PostgreSQL\\16\\data",
  linux: "/var/lib/postgresql/16/main",
};

export const localLabel: Record<Os, string> = { macos: "This Mac", windows: "This PC", linux: "This PC" };

/** Every machine for one OS choice. WSL shows only on Windows. */
export function demoMachines(os: Os, at = Date.now()): Machine[] {
  const local = localPaths(shifted(fixtureSnapshot, at), os);
  const withSystem = (servers: Server[], memory_total: number, memory_used: number, cpu_percent: number): Snapshot => ({
    taken_at: at,
    system: { memory_total, memory_used, memory_other_apps: memory_used / 2, cpu_percent },
    servers,
    other_ports: [],
  });
  const GB = 1024 * MB;

  const machines: Machine[] = [
    {
      id: LOCAL,
      label: localLabel[os],
      host: host(os === "macos" ? "mac" : "pc", os, os === "macos" ? "aarch64" : "x86_64", 10),
      state: "connected",
      snapshot: {
        ...local,
        // Postgres moves from "other ports" to a protected server of our own.
        servers: [...local.servers, postgres(at, POSTGRES_DIR[os])].sort(
          (a, b) => a.port - b.port,
        ),
        other_ports: local.other_ports.filter((port) => port.port !== 5432),
      },
    },
    {
      id: "devbox",
      label: "devbox",
      host: host("devbox", "linux", "x86_64", 16),
      state: "connected",
      snapshot: withSystem(
        [
          server(
            {
              port: 3000,
              name: "billing-api",
              framework: "Rails",
              branch: "invoices-v2",
              cwd: "/home/dev/billing-api",
              command: "bin/rails server",
              processes: [["bin/rails server", 42, 0.2], ["puma 6.4.2 (tcp://0.0.0.0:3000)", 486, 3.4]],
              up: 26 * HOUR,
            },
            at,
          ),
          server(
            {
              port: 3036,
              name: "billing-web",
              framework: "Vite",
              branch: "invoices-v2",
              cwd: "/home/dev/billing-api",
              command: "bin/vite dev",
              processes: [["bin/vite dev", 31, 0], ["vite", 164, 0.8]],
              up: 26 * HOUR,
              idle: 20 * HOUR,
            },
            at,
          ),
          postgres(at, "/var/lib/postgresql/16/main"),
          server(
            {
              port: 6379,
              name: "redis",
              framework: null,
              branch: null,
              cwd: "/var/lib/redis",
              command: "redis-server *:6379",
              processes: [["redis-server *:6379", 14, 0.2]],
              up: 9 * 24 * HOUR,
              protected: true,
            },
            at,
          ),
        ],
        32 * GB,
        11 * GB,
        9,
      ),
    },
    {
      id: "wsl",
      label: "Ubuntu",
      host: host("ubuntu", "linux"),
      state: "connected",
      snapshot: withSystem(
        [
          server(
            {
              port: 3000,
              name: "storefront",
              framework: "Next.js",
              branch: "checkout-redesign",
              cwd: "/home/dev/storefront",
              command: "pnpm dev",
              processes: [["pnpm dev", 48, 0], ["next-server (v16.1)", 914, 7.1], ["turbopack", 188, 1.9]],
              up: 2 * HOUR,
              agent: {
                kind: "codex",
                id: "0199a3c1-7e2b-7c40-b1d2-5f6e7a8b9c0d",
                title: "Checkout page redesign",
                started_at: at - 2.5 * HOUR,
                transcript_path: "/home/dev/.codex/sessions/2026/09/29/rollout-0199a3c1.jsonl",
                directory: "/home/dev/storefront",
                resume_command: "codex resume 0199a3c1-7e2b-7c40-b1d2-5f6e7a8b9c0d",
              },
            },
            at,
          ),
          server(
            {
              port: 6006,
              name: "storefront-ui",
              framework: "Storybook",
              branch: "main",
              cwd: "/home/dev/storefront/packages/ui",
              command: "storybook dev -p 6006",
              processes: [["storybook dev -p 6006", 402, 1.2]],
              up: 5 * 24 * HOUR,
              idle: 3 * 24 * HOUR,
            },
            at,
          ),
        ],
        16 * GB,
        9 * GB,
        12,
      ),
    },
    {
      id: "docker",
      label: "api-container",
      host: host("3f9c2a1b7d4e", "linux", "aarch64", 4),
      state: "connected",
      snapshot: withSystem(
        [
          server(
            {
              port: 8000,
              name: "orders",
              framework: "FastAPI",
              branch: null,
              cwd: "/app",
              command: "uvicorn orders.main:app --reload --host 0.0.0.0",
              processes: [["uvicorn orders.main:app --reload", 58, 0.1], ["python3 (reloader worker)", 176, 2.6]],
              up: 4 * HOUR,
            },
            at,
          ),
          server(
            {
              port: 5555,
              name: "flower",
              framework: null,
              branch: null,
              cwd: "/app",
              command: "celery -A orders flower",
              processes: [["celery -A orders flower", 96, 0.4]],
              up: 4 * HOUR,
            },
            at,
          ),
        ],
        8 * GB,
        3 * GB,
        6,
      ),
    },
  ];
  return os === "windows" ? machines : machines.filter((machine) => machine.id !== "wsl");
}

/** How the CLI names each machine, for the `ppm --on` line. */
export const cliName: Record<string, string | null> = { local: null, devbox: "devbox", wsl: "Ubuntu", docker: "api-container" };
