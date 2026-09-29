// An in-page PpmClient over the demo machines. Stop and Clean up remove rows,
// restart waits like a real one, and the rest only says what the app would do.
import type { Machine, PpmClient, Snapshot } from "@ppm/protocol";

type Listener = (machines: Machine[]) => void;

export type DemoClient = PpmClient & {
  /** What the app would have done, such as "Opens localhost:3000", for the page to show. */
  onNotice(listener: (text: string) => void): () => void;
  /** Whether anything was stopped since the last reset. */
  changed(): boolean;
  reset(machines: Machine[]): void;
  /** A client that shows only one machine, so the page's own switcher picks it. */
  only(machineId: string): PpmClient;
};

const TICK = 2000;

/** Moves every server's history forward one scan, with a little noise, like a live sidecar. */
function tick(snapshot: Snapshot): Snapshot {
  const taken_at = snapshot.taken_at + TICK;
  return {
    ...snapshot,
    taken_at,
    servers: snapshot.servers.map((server) => {
      if (!server.history.length || server.status === "idle") return server;
      const drift = server.status === "attention" ? 1.004 : 1 + (Math.random() - 0.5) * 0.02;
      const memory = server.memory * drift;
      const cpu_percent = Math.max(0, server.cpu_percent + (Math.random() - 0.5) * 3);
      const history = [...server.history.filter((sample) => sample.at > taken_at - 600_000), { at: taken_at, memory, cpu_percent }];
      return { ...server, memory, cpu_percent, history };
    }),
  };
}

export function demoClient(initial: Machine[], live: boolean): DemoClient {
  let machines = initial;
  let stopped = false;
  const listeners = new Set<Listener>();
  const notices = new Set<(text: string) => void>();
  const emit = () => listeners.forEach((listener) => listener(machines));
  const notice = (text: string) => notices.forEach((listener) => listener(text));
  const label = (id: string) => machines.find((machine) => machine.id === id)?.label ?? id;
  const update = (id: string, change: (snapshot: Snapshot) => Snapshot) => {
    machines = machines.map((machine) => (machine.id === id && machine.snapshot ? { ...machine, snapshot: change(machine.snapshot) } : machine));
    emit();
  };

  if (live) {
    setInterval(() => {
      if (document.hidden) return;
      machines = machines.map((machine) => (machine.snapshot ? { ...machine, snapshot: tick(machine.snapshot) } : machine));
      emit();
    }, TICK);
  }

  const client: DemoClient = {
    machines: () => machines,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async call(machineId, call) {
      if (call.method === "stop") {
        const { port } = call.params;
        await new Promise((resolve) => setTimeout(resolve, 350));
        stopped = true;
        update(machineId, (snapshot) => ({ ...snapshot, servers: snapshot.servers.filter((server) => server.port !== port) }));
      } else if (call.method === "restart") {
        await new Promise((resolve) => setTimeout(resolve, 900));
        notice(`Restarted :${call.params.port}`);
      }
    },
    openUrl: async (machineId, port) =>
      notice(machineId === "local" ? `Opens localhost:${port} in your browser` : `Forwards :${port} from ${label(machineId)} and opens it`),
    openWorkspace: async (_, port) => notice(`Opens the workspace running :${port}`),
    openExternal: async (url) => notice(`Opens ${new URL(url).host}`),
    revealFolder: async (_, path) => notice(`Shows ${path} in your file manager`),
    openInEditor: async (_, path) => notice(`Opens ${path} in your editor`),
    resumeSession: async (_, session) => notice(`Runs ${session.resume_command} in your terminal`),
    openSettings: () => notice("Opens Settings"),
    onNotice(listener) {
      notices.add(listener);
      return () => notices.delete(listener);
    },
    changed: () => stopped,
    reset(next) {
      machines = next;
      stopped = false;
      emit();
    },
    only(machineId) {
      const pick = (all: Machine[]) => all.filter((machine) => machine.id === machineId);
      return {
        ...client,
        machines: () => pick(machines),
        subscribe: (listener) => client.subscribe((all) => listener(pick(all))),
      };
    },
  };
  return client;
}
