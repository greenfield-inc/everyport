// Machines for the playground, built from the protocol fixture.
import { fixtureSnapshot, type Machine, type EveryportClient, type Server, type Snapshot } from "@everyport/protocol";

const host = (hostname: string, os: "macos" | "linux" | "windows") => ({ hostname, os, arch: "aarch64", cores: 10 });

function protectedServer(base: Server, port: number, name: string): Server {
  return {
    ...base,
    port,
    process_name: name,
    protected: true,
    status: "running",
    clean_up: null,
    agent: null,
    workspace: null,
    project: { ...base.project, name, framework: null, branch: null, worktree: null, github: null, vercel: null },
  };
}

const withServers = (servers: Server[]): Snapshot => ({ ...fixtureSnapshot, servers });

const local = (snapshot: Snapshot | null): Machine => ({
  id: "local",
  label: "This Mac",
  host: host("mac", "macos"),
  state: "connected",
  snapshot,
});

const [nextApp, site, , storybook] = fixtureSnapshot.servers;

export const scenarios: Record<string, () => Machine[]> = {
  /** The Paper design's state. */
  paper: () => [local(fixtureSnapshot)],
  /** Paper plus a database and a cache, which Clean up leaves alone. */
  protected: () => [
    local(
      withServers([
        ...fixtureSnapshot.servers,
        protectedServer(site, 5432, "postgres"),
        protectedServer(site, 6379, "redis"),
      ]),
    ),
  ],
  machines: () => [
    local(fixtureSnapshot),
    {
      id: "devbox",
      label: "devbox",
      host: host("devbox", "linux"),
      state: "connected",
      snapshot: withServers([
        { ...nextApp, port: 8080, workspace: null, agent: null },
        { ...storybook, port: 9000 },
      ]),
    },
    { id: "wsl-ubuntu", label: "Ubuntu", host: null, state: "connecting", snapshot: null },
    { id: "gpu", label: "gpu-01", host: null, state: "error", error: "ssh: connect to host gpu-01 port 22: Connection refused", snapshot: null },
  ],
  empty: () => [local(withServers([]))],
  /** 50 servers and 12 other ports, for scrolling and update cost. */
  many: () => [
    local({
      ...withServers(Array.from({ length: 50 }, (_, index) => ({ ...fixtureSnapshot.servers[index % 5], port: 3000 + index }))),
      other_ports: Array.from({ length: 12 }, (_, index) => ({ ...fixtureSnapshot.other_ports[1], port: 5432 + index })),
    }),
  ],
};

/** Moves every server's history forward one scan, with a little noise. */
function tick(snapshot: Snapshot, step: number): Snapshot {
  const taken_at = snapshot.taken_at + step;
  return {
    ...snapshot,
    taken_at,
    servers: snapshot.servers.map((server) => {
      const memory = Math.max(1, server.memory * (1 + (Math.random() - 0.5) * 0.02));
      const cpu_percent = Math.max(0, server.cpu_percent + (Math.random() - 0.5) * 4);
      const history = server.history.length
        ? [...server.history.filter((sample) => sample.at > taken_at - 600_000), { at: taken_at, memory, cpu_percent }]
        : server.history;
      return { ...server, memory, cpu_percent, history };
    }),
  };
}

/**
 * An EveryportClient over in-memory machines. Stop removes the server, and `live`
 * advances every snapshot on an interval like a real sidecar.
 */
export function fixtureClient(machines: Machine[], live: number | null): EveryportClient & { emit: () => void } {
  const listeners = new Set<(machines: Machine[]) => void>();
  const log = (...args: unknown[]) => console.info("[everyport]", ...args);
  const emit = () => {
    machines = [...machines];
    for (const listener of listeners) listener(machines);
  };
  const update = (id: string, change: (snapshot: Snapshot) => Snapshot) => {
    machines = machines.map((machine) => (machine.id === id && machine.snapshot ? { ...machine, snapshot: change(machine.snapshot) } : machine));
    emit();
  };
  if (live) {
    setInterval(() => {
      for (const machine of machines) update(machine.id, (snapshot) => tick(snapshot, live));
    }, live);
  }
  return {
    emit: () => {
      for (const machine of machines) update(machine.id, (snapshot) => tick(snapshot, 2000));
    },
    machines: () => machines,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async call(machineId, call) {
      log("call", machineId, call);
      if (call.method === "stop") {
        const { port } = call.params;
        setTimeout(() => update(machineId, (snapshot) => ({ ...snapshot, servers: snapshot.servers.filter((server) => server.port !== port) })), 400);
      }
      if (call.method === "restart") await new Promise((resolve) => setTimeout(resolve, 900));
    },
    openUrl: async (machineId, port) => log("openUrl", machineId, port),
    openWorkspace: async (machineId, port) => log("openWorkspace", machineId, port),
    openExternal: async (url) => log("openExternal", url),
    revealFolder: async (machineId, path) => log("revealFolder", machineId, path),
    openInEditor: async (machineId, path) => log("openInEditor", machineId, path),
    resumeSession: async (machineId, session) => log("resumeSession", machineId, session.resume_command),
    openSettings: () => log("openSettings"),
  };
}
