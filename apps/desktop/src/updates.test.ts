import { fixtureSnapshot, type Machine, type Server } from "@ppm/protocol";
import { describe, expect, it, vi } from "vitest";
import { type Base, MachineUpdates, type Update } from "./updates";

const machine = (servers: unknown[], taken_at = fixtureSnapshot.taken_at) => ({
  id: "local",
  label: "mac",
  host: null,
  state: "connected" as const,
  snapshot: { ...fixtureSnapshot, taken_at, servers },
});
const base: Base = { seq: 7, machines: [machine(fixtureSnapshot.servers) as Machine] };
const [web, api, ...rest] = fixtureSnapshot.servers;
const patch = (fields: Partial<Server> & Pick<Server, "port">) => ({ patch: fields });
const bare = ({ port }: Server) => patch({ port });
const update = (seq: number, servers: unknown[], taken_at?: number) => ({ seq, machines: [machine(servers, taken_at)] }) as Update;

/** Updates started from the first of `bases`, with each later `machines_sync` answered by the next. */
async function started(...bases: Base[]) {
  const sync = vi.fn(async () => bases.shift()!);
  const updates = new MachineUpdates(sync, () => {});
  updates.resync();
  await vi.waitFor(() => expect(updates.machines).not.toEqual([]));
  return { updates, sync };
}
const servers = (updates: MachineUpdates) => updates.machines[0].snapshot!.servers;

describe("MachineUpdates", () => {
  it("applies changed fields and the newest samples to the servers the page has", async () => {
    const { updates } = await started(base);
    const newest = web.history[web.history.length - 1];
    // 10 s later: next-server grew, :3000 reached 1.5 GB, its newest sample took that, and a new one started.
    const now = fixtureSnapshot.taken_at + 10_000;
    const tail = [
      { ...newest, memory: 1_500_000_000 },
      { at: now, memory: 1_500_000_000, cpu_percent: 4 },
    ];
    const [npm, next, turbopack] = web.processes;
    updates.receive(
      update(
        8,
        [
          patch({
            port: web.port,
            memory: 1_500_000_000,
            processes: [{ proc: npm.proc }, { proc: next.proc, memory: 1_200_000_000 }, { proc: turbopack.proc }] as Server["processes"],
            history: tail,
          }),
          bare(api),
          ...rest.map(bare),
        ],
        now,
      ),
    );

    expect(servers(updates)).toEqual([
      {
        ...web,
        memory: 1_500_000_000,
        processes: [npm, { ...next, memory: 1_200_000_000 }, turbopack],
        // The first sample is now more than 10 minutes old.
        history: [...web.history.slice(1, -1), ...tail],
      },
      api,
      ...rest,
    ]);
    expect(updates.machines[0].snapshot!.taken_at).toBe(now);
  });

  it("keeps history and processes when only the project changed", async () => {
    const { updates } = await started(base);
    // `git checkout` on :3001.
    const project = { ...api.project, branch: "fix/login" };
    updates.receive(update(8, [bare(web), patch({ port: api.port, project }), ...rest.map(bare)]));

    expect(servers(updates)[1]).toEqual({ ...api, project });
  });

  it("replaces a server that comes in full", async () => {
    const { updates } = await started(base);
    const restarted = { ...api, root: { ...api.root, pid: api.root.pid + 1 }, agent: null };
    updates.receive(update(8, [bare(web), { full: restarted }, ...rest.map(bare)]));

    expect(servers(updates)[1]).toEqual(restarted);
  });

  it("starts over from a new base after a missed update", async () => {
    const project = { ...api.project, branch: "fix/login" };
    // Update 8 had the checkout and never arrived. The new base includes it.
    const after: Base = { seq: 8, machines: [machine([web, { ...api, project }, ...rest]) as Machine] };
    const { updates, sync } = await started(base, after);

    updates.receive(update(9, [bare(web), patch({ port: api.port, connections: 9 }), ...rest.map(bare)]));
    expect(sync).toHaveBeenCalledTimes(2);
    await vi.waitFor(() => expect(servers(updates)[1]).toEqual({ ...api, project, connections: 9 }));

    // An update the base already covers changes nothing.
    updates.receive(update(8, [bare(web), patch({ port: api.port, connections: 1 }), ...rest.map(bare)]));
    expect(servers(updates)[1].connections).toBe(9);
  });
});
