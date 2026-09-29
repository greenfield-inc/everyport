import type { Machine, ProcRef, Sample, Server, ServerProcess } from "@ppm/protocol";

/** How far back `Server.history` reaches, as in the engine. */
const HISTORY_MS = 10 * 60 * 1000;

/**
 * Applies a `machines` update from updates.rs. A server arrives in full the
 * first time, and again for a new process on its port. After that it
 * carries its `port` and the fields that changed, with only the newest
 * samples of `history`, and each process as its `proc` and what changed. Calls `resync` when an update changes a server this
 * page doesn't have, and leaves that server out until it does.
 */
export function merge(current: Machine[], update: Machine[], resync: () => void): Machine[] {
  return update.map((machine) => {
    if (!machine.snapshot) return machine;
    const known = new Map(current.find((m) => m.id === machine.id)?.snapshot?.servers.map((s) => [s.port, s]));
    const { taken_at } = machine.snapshot;
    const servers = machine.snapshot.servers.flatMap((change: Partial<Server> & Pick<Server, "port">) => {
      // A server in full always has its project.
      if (change.project) return [change as Server];
      const old = known.get(change.port);
      if (!old) {
        resync();
        return [];
      }
      return [
        {
          ...old,
          ...change,
          history: change.history ? join(old.history, change.history, taken_at) : old.history,
          processes: change.processes ? processes(old.processes, change.processes) : old.processes,
        },
      ];
    });
    return { ...machine, snapshot: { ...machine.snapshot, servers } };
  });
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
