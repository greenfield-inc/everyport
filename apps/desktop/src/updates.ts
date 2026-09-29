import type { Machine, ProcRef, Sample, Server, ServerProcess } from "@ppm/protocol";

/** How far back `Server.history` reaches, as in the engine. */
const HISTORY_MS = 10 * 60 * 1000;

/**
 * A server in an update from updates.rs: in full, or its `port` and the
 * fields that changed. A patch's `history` holds only the newest samples, and
 * its `processes` hold each process's `proc` and what changed.
 */
type ServerUpdate = { full: Server } | { patch: Partial<Server> & Pick<Server, "port"> };
type MachineUpdate = Omit<Machine, "snapshot"> & {
  snapshot: (Omit<NonNullable<Machine["snapshot"]>, "servers"> & { servers: ServerUpdate[] }) | null;
};
export type Update = { seq: number; machines: MachineUpdate[] };
/** Every machine in full, from `machines_sync`, and the `seq` the next update follows. */
export type Base = { seq: number; machines: Machine[] };

/**
 * The machines as updates.rs sends them. Updates apply in `seq` order. After a
 * missed one, it asks for a new base through `sync` and holds updates until it
 * arrives.
 */
export class MachineUpdates {
  machines: Machine[] = [];
  #seq: number | null = null;
  #held: Update[] = [];
  #syncing = false;
  readonly #sync: () => Promise<Base>;
  readonly #changed: (machines: Machine[]) => void;

  constructor(sync: () => Promise<Base>, changed: (machines: Machine[]) => void) {
    this.#sync = sync;
    this.#changed = changed;
  }

  receive(update: Update) {
    if (this.#seq !== null) {
      // Already applied, or in the base.
      if (update.seq <= this.#seq) return;
      const machines = update.seq === this.#seq + 1 ? merge(this.machines, update.machines) : null;
      if (machines) {
        this.#seq = update.seq;
        return this.#set(machines);
      }
    }
    this.#held.push(update);
    this.resync();
  }

  /** Asks for a new base, unless one is on its way. */
  resync() {
    this.#seq = null;
    if (this.#syncing) return;
    this.#syncing = true;
    this.#sync().then(
      (base) => {
        this.#syncing = false;
        this.#seq = base.seq;
        this.#set(base.machines);
        const held = this.#held;
        this.#held = [];
        for (const update of held) this.receive(update);
      },
      () => {
        // The next update asks again.
        this.#syncing = false;
      },
    );
  }

  #set(machines: Machine[]) {
    this.machines = machines;
    this.#changed(machines);
  }
}

/** Applies an update to `current`, or null when it patches a server `current` doesn't have. */
export function merge(current: Machine[], update: MachineUpdate[]): Machine[] | null {
  const machines: Machine[] = [];
  for (const machine of update) {
    if (!machine.snapshot) {
      machines.push({ ...machine, snapshot: null });
      continue;
    }
    const known = new Map(current.find((m) => m.id === machine.id)?.snapshot?.servers.map((s) => [s.port, s]));
    const { taken_at } = machine.snapshot;
    const servers: Server[] = [];
    for (const change of machine.snapshot.servers) {
      if ("full" in change) {
        servers.push(change.full);
        continue;
      }
      const { patch } = change;
      const old = known.get(patch.port);
      if (!old) return null;
      servers.push({
        ...old,
        ...patch,
        history: patch.history ? join(old.history, patch.history, taken_at) : old.history,
        processes: patch.processes ? processes(old.processes, patch.processes) : old.processes,
      });
    }
    machines.push({ ...machine, snapshot: { ...machine.snapshot, servers } });
  }
  return machines;
}

/** The old samples before the new ones start, then the new ones, within the window. */
function join(old: Sample[], newest: Sample[], now: number): Sample[] {
  const from = newest[0]?.at ?? Infinity;
  return [...old.filter((s) => s.at < from), ...newest].filter((s) => now - s.at <= HISTORY_MS);
}

const id = ({ pid, started_at }: ProcRef) => `${pid}:${started_at}`;

/** A new process comes in full; a known one gets what changed. */
function processes(old: ServerProcess[], changes: ServerProcess[]): ServerProcess[] {
  const known = new Map(old.map((p) => [id(p.proc), p]));
  return changes.map((change) => ({ ...known.get(id(change.proc)), ...change }));
}
