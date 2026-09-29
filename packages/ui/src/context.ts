import type { Machine, PpmClient, ProcRef, Server, Snapshot } from "@ppm/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type Action, DEFAULT_ALERT_MEMORY } from "./model.ts";

/**
 * A request in flight for a port, the error it came back with, or a stop or
 * restart of a protected server waiting for the user to confirm it. A confirm
 * holds the server's root, so it never carries over to a new process.
 */
export type Pending = "stopping" | "restarting" | { error: string } | { confirm: Action; root: ProcRef };

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
  /**
   * Stop and restart ask first for a protected server, and only `confirm`
   * goes ahead: repeating the action just asks again. They return false
   * while waiting for that answer.
   */
  stop: (server: Server, force?: boolean) => boolean;
  restart: (server: Server) => boolean;
  confirm: (server: Server) => void;
  /** Dismisses every waiting confirm. */
  cancel: () => void;
  /** Runs a host action for a server, such as revealing its folder, and shows its error on the server. */
  act: (server: Server, action: () => Promise<void> | void) => void;
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

  // A stopped server leaves the next snapshot; forget what was pending for it,
  // and drop a confirm whose port now belongs to another process.
  useEffect(() => {
    const live = new Map(snapshot.servers.map((server) => [server.port, server.root]));
    setPending((current) => {
      const stale = [...current]
        .filter(([port, value]) => {
          const root = live.get(port);
          const confirming = typeof value === "object" && "confirm" in value;
          return !root || (confirming && !sameProc(value.root, root));
        })
        .map(([port]) => port);
      if (!stale.length) return current;
      const next = new Map(current);
      for (const port of stale) next.delete(port);
      return next;
    });
  }, [snapshot]);

  // An error shows on its server for a few seconds.
  const fail = useCallback(
    (port: number) => (error: unknown) => {
      const shown = { error: String(error instanceof Error ? error.message : error) };
      settle(port, shown);
      setTimeout(() => setPending((current) => (current.get(port) === shown ? new Map([...current].filter(([key]) => key !== port)) : current)), 6000);
    },
    [settle],
  );

  return useMemo(() => {
    const confirming = (server: Server) => {
      const current = pending.get(server.port);
      return typeof current === "object" && "confirm" in current && sameProc(current.root, server.root) ? current.confirm : null;
    };
    /** Holds a protected server's action until confirmed; true when it may go ahead. */
    const allowed = (server: Server, action: Action, confirmed: boolean) => {
      if (!server.protected || confirmed) return true;
      settle(server.port, { confirm: action, root: server.root });
      return false;
    };
    const stop = (server: Server, force = false, confirmed = false) => {
      if (!allowed(server, force ? "force stop" : "stop", confirmed)) return false;
      settle(server.port, "stopping");
      const params = { port: server.port, root: server.root, force, confirm_protected: server.protected };
      client.call(machineId, { method: "stop", params }).catch(fail(server.port));
      return true;
    };
    const restart = (server: Server, confirmed = false) => {
      if (!allowed(server, "restart", confirmed)) return false;
      settle(server.port, "restarting");
      const params = { port: server.port, root: server.root, confirm_protected: server.protected };
      client.call(machineId, { method: "restart", params }).then(() => settle(server.port, null), fail(server.port));
      return true;
    };
    return {
      client,
      machineId,
      snapshot,
      now: snapshot.taken_at,
      alertMemory,
      colorOf,
      pending,
      open: (server) => void client.openUrl(machineId, server.port).catch(fail(server.port)),
      act: (server, action) => {
        new Promise<void>((resolve) => resolve(action())).catch(fail(server.port));
      },
      stop: (server, force) => stop(server, force),
      restart: (server) => restart(server),
      confirm: (server) => {
        const action = confirming(server);
        if (action === "restart") restart(server, true);
        else if (action) stop(server, action === "force stop", true);
      },
      cancel: () => {
        for (const [port, value] of pending) if (confirmOf(value)) settle(port, null);
      },
    };
  }, [client, machineId, snapshot, alertMemory, colorOf, pending, settle, fail]);
}

const sameProc = (a: ProcRef, b: ProcRef) => a.pid === b.pid && a.started_at === b.started_at;

/** The action a pending confirm is for, if any. */
export const confirmOf = (pending: Pending | undefined) => (typeof pending === "object" && "confirm" in pending ? pending.confirm : null);

/** The error a request came back with, if any. */
export const errorOf = (pending: Pending | undefined) => (typeof pending === "object" && "error" in pending ? pending.error : null);

export const copy = (text: string) => void navigator.clipboard?.writeText(text);
