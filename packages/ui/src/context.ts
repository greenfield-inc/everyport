import type { Machine, PpmClient, Server, Snapshot } from "@ppm/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DEFAULT_ALERT_MEMORY } from "./model.ts";

/** A request in flight for a port, or the error it came back with. */
export type Pending = "stopping" | "restarting" | { error: string };

/** Everything a view needs about one machine, and the actions on its servers. */
export type ViewContext = {
  client: PpmClient;
  machineId: string;
  snapshot: Snapshot;
  /** `snapshot.taken_at`: views measure uptime and idle time from it. */
  now: number;
  alertMemory: number;
  /** A port's color, stable while the server runs. */
  colorOf: (port: number) => string;
  pending: ReadonlyMap<number, Pending>;
  open: (server: Server) => void;
  stop: (server: Server, force?: boolean) => void;
  restart: (server: Server) => void;
};

const CHART_COLORS = 5;

/**
 * Gives each port one of the theme's five chart colors, the lowest one free,
 * and keeps it while the server runs, so stopping one server never recolors
 * the others.
 */
function usePortColors(servers: Server[]) {
  const assigned = useRef(new Map<number, number>());
  const map = assigned.current;
  const live = new Set(servers.map((server) => server.port));
  for (const port of map.keys()) if (!live.has(port)) map.delete(port);
  for (const { port } of servers) {
    if (map.has(port)) continue;
    const used = new Set(map.values());
    let index = 0;
    while (used.has(index)) index++;
    map.set(port, index);
  }
  return useCallback((port: number) => `var(--chart-${((map.get(port) ?? 0) % CHART_COLORS) + 1})`, [map]);
}

export function useViewContext(client: PpmClient, machine: Machine & { snapshot: Snapshot }, alertMemory = DEFAULT_ALERT_MEMORY): ViewContext {
  const { snapshot, id: machineId } = machine;
  const [pending, setPending] = useState<ReadonlyMap<number, Pending>>(new Map());
  const colorOf = usePortColors(snapshot.servers);

  const settle = useCallback((port: number, value: Pending | null) => {
    setPending((current) => {
      const next = new Map(current);
      if (value) next.set(port, value);
      else next.delete(port);
      return next;
    });
  }, []);

  // A stopped server leaves the next snapshot; forget what was pending for it.
  useEffect(() => {
    const live = new Set(snapshot.servers.map((server) => server.port));
    setPending((current) => {
      const stale = [...current.keys()].filter((port) => !live.has(port));
      if (!stale.length) return current;
      const next = new Map(current);
      for (const port of stale) next.delete(port);
      return next;
    });
  }, [snapshot]);

  const fail = useCallback((port: number) => (error: unknown) => settle(port, { error: String(error instanceof Error ? error.message : error) }), [settle]);

  return useMemo(
    () => ({
      client,
      machineId,
      snapshot,
      now: snapshot.taken_at,
      alertMemory,
      colorOf,
      pending,
      open: (server) => void client.openUrl(machineId, server.port).catch(fail(server.port)),
      stop: (server, force = false) => {
        settle(server.port, "stopping");
        client.call(machineId, { method: "stop", params: { port: server.port, root: server.root, force } }).catch(fail(server.port));
      },
      restart: (server) => {
        settle(server.port, "restarting");
        client
          .call(machineId, { method: "restart", params: { port: server.port, root: server.root } })
          .then(() => settle(server.port, null), fail(server.port));
      },
    }),
    [client, machineId, snapshot, alertMemory, colorOf, pending, settle, fail],
  );
}

export const copy = (text: string) => void navigator.clipboard?.writeText(text);
