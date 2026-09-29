// Each view rendered from the protocol fixture. Expected text comes from the
// Paper frames in docs/design, which show the same servers.
import { fixtureSnapshot, type Machine, type PpmClient, type Server, type Snapshot } from "@ppm/protocol";
import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { alertText, CleanUp, cleanUpCandidates, Popover, useViewContext, type ViewContext } from "./index.ts";

const machine = (snapshot: Snapshot): Machine & { snapshot: Snapshot } => ({
  id: "local",
  label: "This Mac",
  host: null,
  state: "connected",
  snapshot,
});

const clientFor = (machines: Machine[]): PpmClient => ({
  machines: () => machines,
  subscribe: () => () => {},
  call: async () => {},
  openUrl: async () => {},
});

/** Rendered text, with tags removed and whitespace collapsed. */
const text = (node: ReactNode) =>
  renderToStaticMarkup(node)
    .replace(/<[^>]+>/g, " ")
    .replace(/&#x27;/g, "'")
    .replace(/\s+/g, " ");

const server = (port: number) => fixtureSnapshot.servers.find((candidate) => candidate.port === port) as Server;

function WithContext({ snapshot, children }: { snapshot: Snapshot; children: (ctx: ViewContext) => ReactNode }) {
  const m = machine(snapshot);
  return children(useViewContext(clientFor([m]), m));
}

describe("server list (Paper 01)", () => {
  const list = text(<Popover client={clientFor([machine(fixtureSnapshot)])} />);

  it("totals server memory and shows the machine's CPU", () => {
    expect(list).toContain("Servers 4.9 GB CPU 18%");
  });

  it("gives each row its memory, and a context line from its workspace, folder and times", () => {
    expect(list).toContain("3000 port-process-manager menubar-port-monitor providence · up 3h 1.24 GB");
    expect(list).toContain("3001 greenfield.to main ~/Sites · up 1d 612 MB");
    expect(list).toContain("lisbon · idle 5h 184 MB");
    expect(list).toContain("+1.1 GB in 10 min 2.81 GB");
    expect(list).toContain("Worktree deleted · idle 2d 96 MB");
  });

  it("counts the servers Clean up would stop, leaving out the leaking one", () => {
    expect(list).toContain("Clean up 2");
  });

  it("shows a switcher only when more than one machine is connected", () => {
    expect(list).not.toContain("This Mac");
    const devbox = { ...machine(fixtureSnapshot), id: "devbox", label: "devbox" };
    expect(text(<Popover client={clientFor([machine(fixtureSnapshot), devbox])} />)).toContain("This Mac 5 devbox 5");
  });
});

describe("server detail (Paper 02)", () => {
  const detail = text(<Popover client={clientFor([machine(fixtureSnapshot)])} initialServer={{ machineId: "local", port: 3000 }} />);

  it("shows the port, uptime and session", () => {
    expect(detail).toContain("3000 up 3h 12m");
    expect(detail).toContain("Session Dot-grid menu bar icon 68c8fda6");
    expect(detail).toContain("Branch menubar-port-monitor");
  });

  it("puts six more facts behind a disclosure, in Paper's order", () => {
    expect(detail).toContain(
      "Workspace Conductor · providence Folder ~/…/port-process-manager/providence Framework Next.js Command npm run dev Started Today 10:12 AM Address 127.0.0.1 · ::1 6 more",
    );
  });

  it("summarizes memory, CPU and the process tree", () => {
    expect(detail).toContain("Memory 1.24 GB 10 min");
    expect(detail).toContain("CPU 12%");
    expect(detail).toContain("Processes 3 · 1.24 GB");
    expect(detail).toContain("Open localhost:3000");
  });

  it("falls back to the list when the server is gone", () => {
    const gone = text(<Popover client={clientFor([machine(fixtureSnapshot)])} initialServer={{ machineId: "local", port: 4321 }} />);
    expect(gone).toContain("Servers 4.9 GB");
  });
});

describe("clean up (Paper 03)", () => {
  const withDatabases: Snapshot = {
    ...fixtureSnapshot,
    servers: [
      ...fixtureSnapshot.servers,
      { ...server(3001), port: 5432, process_name: "postgres", protected: true },
      { ...server(3001), port: 6379, process_name: "redis", protected: true, clean_up: { kind: "idle", seconds: 90000 } },
    ],
  };
  const suggested = cleanUpCandidates(withDatabases.servers);

  it("offers deleted worktrees first and never offers protected servers", () => {
    expect(suggested.map((candidate) => candidate.port)).toEqual([8000, 5173, 6006]);
  });

  it("adds up what the checked servers free, and names the protected ones", () => {
    const view = text(
      <WithContext snapshot={withDatabases}>
        {(ctx) => <CleanUp ctx={ctx} checked={new Set([8000, 5173])} selected={null} onToggle={() => {}} onBack={() => {}} />}
      </WithContext>,
    );
    expect(view).toContain("~280 MB can be freed by stopping 2 servers");
    expect(view).toContain("Idle 5h · no connections");
    expect(view).toContain("Leaking · +1.1 GB in 10 min");
    expect(view).toContain("postgres :5432 and redis :6379 are protected");
    expect(view).toContain("Stop 2 servers · free 280 MB");
  });
});

describe("notification (Paper 04)", () => {
  it("says what leaked, by how much, and how much it uses now", () => {
    const leaking = server(6006);
    expect(alertText(leaking, { port: 6006, kind: "leaking", memory: leaking.memory })).toEqual({
      title: ":6006 design-system is leaking",
      body: "Storybook grew 1.1 GB in 10 minutes and is now using 2.8 GB.",
    });
  });
});
