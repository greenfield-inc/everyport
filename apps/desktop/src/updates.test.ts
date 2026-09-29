import { fixtureSnapshot, type Machine, type Server } from "@ppm/protocol";
import { describe, expect, it, vi } from "vitest";
import { merge } from "./updates";

const machine = (servers: unknown[], taken_at = fixtureSnapshot.taken_at): Machine => ({
  id: "local",
  label: "mac",
  host: null,
  state: "connected",
  snapshot: { ...fixtureSnapshot, taken_at, servers: servers as Server[] },
});
const full = [machine(fixtureSnapshot.servers)];
const bare = ({ port }: Server) => ({ port });

describe("merge", () => {
  it("applies changed fields and the newest samples to the servers the page has", () => {
    const [web, ...rest] = fixtureSnapshot.servers;
    const newest = web.history[web.history.length - 1];
    // 10 s later: next-server grew, :3000 reached 1.5 GB, its newest sample took that, and a new one started.
    const now = fixtureSnapshot.taken_at + 10_000;
    const tail = [
      { ...newest, memory: 1_500_000_000 },
      { at: now, memory: 1_500_000_000, cpu_percent: 4 },
    ];
    const [npm, next, turbopack] = web.processes;
    const update = [
      machine(
        [
          {
            ...bare(web),
            memory: 1_500_000_000,
            processes: [{ proc: npm.proc }, { proc: next.proc, memory: 1_200_000_000 }, { proc: turbopack.proc }],
            history: tail,
          },
          ...rest.map(bare),
        ],
        now,
      ),
    ];

    const [merged] = merge(full, update, () => {});
    const servers = merged.snapshot!.servers;
    expect(servers[0]).toEqual({
      ...web,
      memory: 1_500_000_000,
      processes: [npm, { ...next, memory: 1_200_000_000 }, turbopack],
      // The first sample is now more than 10 minutes old.
      history: [...web.history.slice(1, -1), ...tail],
    });
    expect(servers.slice(1)).toEqual(rest);
    expect(merged.snapshot!.taken_at).toBe(now);
  });

  it("takes a server in full, and resyncs on a change to one it doesn't have", () => {
    const resync = vi.fn();
    const [web, api, vite] = fixtureSnapshot.servers;
    // :3001 restarted as another process, which updates.rs sends in full.
    const restarted = { ...api, root: { ...api.root, pid: api.root.pid + 1 }, agent: null };

    const [merged] = merge([machine([web, api])], [machine([bare(web), restarted, { ...bare(vite), memory: 1 }])], resync);

    expect(merged.snapshot!.servers).toEqual([web, restarted]);
    expect(resync).toHaveBeenCalledOnce();
  });
});
