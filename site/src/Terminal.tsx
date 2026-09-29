// A look-alike of `ppm`'s terminal UI over the demo client: arrow keys select,
// Enter opens details, s stops, Esc goes back and q quits to a prompt.
import type { Machine, Os, Server } from "@ppm/protocol";
import { type KeyboardEvent, useState } from "react";
import type { DemoClient } from "./client.ts";

function size(bytes: number) {
  const mb = bytes / 1024 / 1024;
  return mb >= 1000 ? `${(mb / 1024).toFixed(2)} GB` : `${Math.round(mb)} MB`;
}

function uptime(from: number | null, at: number) {
  if (from === null) return "";
  const minutes = Math.round((at - from) / 60_000);
  return minutes < 60 ? `${minutes}m` : minutes < 1440 ? `${Math.round(minutes / 60)}h` : `${Math.round(minutes / 1440)}d`;
}

const fit = (text: string, width: number) => (text.length > width ? `${text.slice(0, width - 1)}…` : text.padEnd(width));
const center = (text: string, width: number) => text.padStart(Math.floor((width + text.length) / 2)).padEnd(width);

type Screen = { view: "list" } | { view: "detail"; port: number } | { view: "shell" };

type Line = { text: string; tone?: "dim" | "selected" | "warn" | "title" | "key" };

function listLines(servers: Server[], selected: number, at: number, width: number): Line[] {
  const used = servers.reduce((sum, server) => sum + server.memory, 0);
  return [
    { text: center("Servers", width), tone: "title" },
    { text: "" },
    { text: `${size(used).padEnd(20)}${`CPU (servers) ${Math.round(servers.reduce((sum, s) => sum + s.cpu_percent, 0))}%`.padStart(width - 20)}` },
    { text: "─".repeat(width), tone: "dim" },
    ...servers.map((server, index): Line => {
      const branch = server.project.branch ? ` ${server.project.branch}` : "";
      const text = `${`:${server.port}`.padEnd(7)}${fit(server.project.name + branch, width - 26)} ${size(server.memory).padStart(8)} ${uptime(server.started_at, at).padStart(5)}${server.protected ? " ⚿" : "  "}`;
      return { text: fit(text, width), tone: index === selected ? "selected" : server.status === "attention" ? "warn" : server.status === "idle" ? "dim" : undefined };
    }),
    ...(servers.length ? [] : [{ text: "  No servers running.", tone: "dim" as const }]),
  ];
}

function detailLines(server: Server, at: number, width: number): Line[] {
  return [
    { text: center(`:${server.port} ${server.project.name}`, width), tone: "title" },
    { text: "" },
    { text: `${size(server.memory)}  CPU ${server.cpu_percent.toFixed(1)}%  up ${uptime(server.started_at, at)}` },
    { text: `${server.project.framework ?? server.process_name}${server.project.branch ? ` on ${server.project.branch}` : ""}`, tone: "dim" },
    ...(server.agent ? [{ text: `${server.agent.kind === "codex" ? "Codex" : "Claude Code"}: ${server.agent.title ?? ""}`, tone: "dim" as const }] : []),
    { text: "─".repeat(width), tone: "dim" },
    { text: "Processes" },
    ...server.processes.map((process): Line => ({ text: fit(`${"  ".repeat(process.depth)}${process.name}`, width - 10) + size(process.memory).padStart(10), tone: "dim" })),
  ];
}

const HINTS: Record<Screen["view"], [string, string][]> = {
  list: [["↑ ↓", "Select"], ["⏎", "Details"], ["s", "Stop"], ["q", "Quit"]],
  detail: [["s", "Stop"], ["esc", "Back"], ["q", "Quit"]],
  shell: [],
};

const TITLE: Record<Os, string> = { macos: "zsh · ppm", windows: "PowerShell", linux: "dev@pc: ~" };

/** `columns` is the terminal's width in characters. */
export function Terminal({ client, machine, os, columns }: { client: DemoClient; machine: Machine | undefined; os: Os; columns: number }) {
  const [screen, setScreen] = useState<Screen>({ view: "list" });
  const [selected, setSelected] = useState(0);
  const servers = machine?.snapshot?.servers ?? [];
  const at = machine?.snapshot?.taken_at ?? Date.now();
  const current = servers[Math.min(selected, servers.length - 1)];
  const detail = screen.view === "detail" ? servers.find((server) => server.port === screen.port) : undefined;
  const shown: Screen = screen.view === "detail" && !detail ? { view: "list" } : screen;

  const stop = (server: Server | undefined) => {
    if (!server || server.protected || !machine) return;
    void client.call(machine.id, { method: "stop", params: { port: server.port, root: server.root, force: false } });
    setScreen({ view: "list" });
  };

  const onKey = (event: KeyboardEvent) => {
    const key = event.key;
    const handled = () => event.preventDefault();
    if (shown.view === "shell") {
      if (key === "Enter") {
        handled();
        setScreen({ view: "list" });
      }
      return;
    }
    if (key === "q") {
      handled();
      setScreen({ view: "shell" });
    } else if (key === "s") {
      handled();
      stop(shown.view === "detail" ? detail : current);
    } else if (shown.view === "list" && (key === "ArrowDown" || key === "j")) {
      handled();
      setSelected(Math.min(servers.length - 1, selected + 1));
    } else if (shown.view === "list" && (key === "ArrowUp" || key === "k")) {
      handled();
      setSelected(Math.max(0, selected - 1));
    } else if (shown.view === "list" && key === "Enter" && current) {
      handled();
      setScreen({ view: "detail", port: current.port });
    } else if (shown.view === "detail" && (key === "Escape" || key === "ArrowLeft")) {
      handled();
      setScreen({ view: "list" });
    }
  };

  const lines =
    shown.view === "shell"
      ? [{ text: "$ ppm" }, { text: "Press Enter to run it again.", tone: "dim" as const }]
      : shown.view === "detail" && detail
        ? detailLines(detail, at, columns)
        : listLines(servers, Math.min(selected, servers.length - 1), at, columns);
  const protectedNote = shown.view !== "shell" && (shown.view === "detail" ? detail : current)?.protected;

  return (
    <div className="terminal" data-os={os}>
      <div className="terminal-bar">
        <span className="lights" aria-hidden>
          <i />
          <i />
          <i />
        </span>
        <span>{TITLE[os]}</span>
      </div>
      <div
        className="terminal-body"
        tabIndex={0}
        role="application"
        aria-label="ppm terminal UI demo. Arrow keys select, Enter opens details, s stops, Escape goes back, q quits."
        onKeyDown={onKey}
      >
        <pre>
          {lines.map((line, index) => (
            <div key={index} className={line.tone}>
              {line.text || " "}
            </div>
          ))}
        </pre>
        <pre className="terminal-hints">
          {protectedNote ? (
            <span className="warn">Protected. ppm stop --protected stops it.</span>
          ) : (
            HINTS[shown.view].map(([key, label]) => (
              <span key={key}>
                <b>{key}</b> {label}
                {"   "}
              </span>
            ))
          )}
          {shown.view === "shell" && <span className="cursor" />}
        </pre>
      </div>
    </div>
  );
}
