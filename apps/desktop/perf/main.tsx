// The popover fed 50 servers every "scan", timed per update. bench.mjs drives it.
//   ?mode=full   every update carries every server in full, as before updates.rs
//   ?mode=delta  updates as updates.rs builds them, applied by MachineUpdates
import { fixtureSnapshot, type Machine, type EveryportClient, type Server } from "@everyport/protocol";
import { Popover } from "@everyport/ui";
import "@everyport/ui/styles.css";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { MachineUpdates, type Update } from "../src/updates";

const params = new URLSearchParams(location.search);
const COUNT = Number(params.get("servers") ?? 50);
const DELTA = params.get("mode") === "delta";

// 50 servers from the fixture, each with 10 minutes of samples every 10 s.
let now = fixtureSnapshot.taken_at;
const servers: Server[] = Array.from({ length: COUNT }, (_, i) => {
  const base = structuredClone(fixtureSnapshot.servers[i % 4]);
  const history = Array.from({ length: 60 }, (_, k) => ({ at: now - (59 - k) * 10_000, memory: base.memory, cpu_percent: k % 7 }));
  return { ...base, port: 39000 + i, root: { pid: 60000 + i, started_at: base.root.started_at }, history };
});

/** One 2 s scan where every process's memory and CPU change: the worst case for updates. */
function scan(): Machine[] {
  now += 2000;
  for (const server of servers) {
    for (const p of server.processes) {
      p.memory = Math.round(p.memory * (0.99 + 0.02 * Math.random()));
      p.cpu_percent = Math.round(Math.random() * 100) / 10;
    }
    server.memory = server.processes.reduce((sum, p) => sum + p.memory, 0);
    server.cpu_percent = server.processes.reduce((sum, p) => sum + p.cpu_percent, 0);
    const last = server.history[server.history.length - 1];
    if (now - last.at >= 10_000) server.history = [...server.history.slice(1), { at: now, memory: server.memory, cpu_percent: server.cpu_percent }];
    else last.memory = server.memory;
  }
  const snapshot = { ...fixtureSnapshot, taken_at: now, servers };
  return structuredClone([{ id: "local", label: "mac", host: null, state: "connected", snapshot }]);
}

// The update updates.rs sends for a scan, built the same way: a server the page
// has as its port and changed fields, history from the page's newest sample on,
// and processes as their proc and changed fields.
type Json = Record<string, unknown>;
let sent = new Map<string, Json>();
let seq = 0;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
function changes(old: Json, now: Json, key: string): Json {
  const out: Json = {};
  for (const [field, value] of Object.entries(now)) {
    if (field === key) out[field] = value;
    else if (same(old[field], value)) continue;
    else if (field === "history") {
      const from = (old.history as { at: number }[]).at(-1)?.at ?? 0;
      out[field] = (value as { at: number }[]).filter((s) => s.at >= from);
    } else if (field === "processes") {
      const id = (p: Json) => JSON.stringify(p.proc);
      const known = new Map((old.processes as Json[]).map((p) => [id(p), p]));
      out[field] = (value as Json[]).map((p) => (known.has(id(p)) ? changes(known.get(id(p))!, p, "proc") : p));
    } else out[field] = value;
  }
  return out;
}
function update(machines: Machine[]): Update {
  const next = new Map<string, Json>();
  const out = machines.map((machine) => ({
    ...machine,
    snapshot: machine.snapshot && {
      ...machine.snapshot,
      servers: machine.snapshot.servers.map((server) => {
        const key = `${server.port}:${server.root.pid}:${server.root.started_at}`;
        const old = sent.get(key);
        next.set(key, server as unknown as Json);
        return old ? { patch: changes(old, server as unknown as Json, "port") as Server } : { full: server };
      }),
    },
  }));
  sent = next;
  return { seq: ++seq, machines: out };
}

const listeners = new Set<(machines: Machine[]) => void>();
const notify = (machines: Machine[]) => {
  for (const listener of listeners) listener(machines);
};
const first = scan();
const updates = new MachineUpdates(async () => ({ seq: 0, machines: first }), notify);
let latest = first;
update(first);
seq = 0;
updates.resync();

const noop = async () => {};
const client: EveryportClient = {
  machines: () => (DELTA ? updates.machines : latest),
  subscribe: (listener) => (listeners.add(listener), () => void listeners.delete(listener)),
  call: noop,
  openUrl: noop,
  openWorkspace: noop,
  openExternal: noop,
  revealFolder: noop,
  openInEditor: noop,
  resumeSession: noop,
};
createRoot(document.getElementById("root")!).render(<Popover client={client} appearance="dark" />);

const stats = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  return { median: sorted[sorted.length >> 1], max: sorted[sorted.length - 1] };
};

/** Runs `count` updates, 50 ms apart, and times each one's parse and render. */
async function bench(count: number) {
  const bytes: number[] = [], parse: number[] = [], render: number[] = [];
  for (let i = 0; i < count; i++) {
    const machines = scan();
    const text = JSON.stringify(DELTA ? update(machines) : machines);
    bytes.push(text.length);
    const t0 = performance.now();
    const payload = JSON.parse(text);
    const t1 = performance.now();
    flushSync(() => {
      if (DELTA) updates.receive(payload);
      else notify((latest = payload));
    });
    void document.body.offsetHeight;
    const t2 = performance.now();
    parse.push(t1 - t0);
    render.push(t2 - t1);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return { bytes: stats(bytes), parse: stats(parse), render: stats(render), rows: document.querySelectorAll("[role=option]").length };
}
Object.assign(window, { bench });
