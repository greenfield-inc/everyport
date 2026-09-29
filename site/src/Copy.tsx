import type { Os } from "@ppm/protocol";
import { useState } from "react";
import { OS_NAMES, RELEASES, REPO, type SectionId } from "./sections.ts";

const RAW = "https://github.com/greenfield-inc/port-process-manager/releases/latest/download";

/** Install commands from the README, per OS. */
const INSTALL: Record<Os, { app: { label: string; command?: string; note: string }; cli: string[] }> = {
  macos: {
    app: { label: "macOS 13 or later", command: "brew install --cask greenfield-inc/tap/port-process-manager", note: "Or download the .dmg. Signed and notarized." },
    cli: [`curl -fsSL ${RAW}/install.sh | sh`, "brew install greenfield-inc/tap/ppm"],
  },
  windows: {
    app: { label: "Windows 10 and 11", command: "winget install Greenfield.PortProcessManager", note: "Or download the .msi. Signed." },
    cli: [`irm ${RAW}/install.ps1 | iex`, "npx port-process-manager"],
  },
  linux: {
    app: { label: "Linux (x86_64)", note: "Download the .deb, .rpm or .AppImage. On GNOME, the tray icon needs the AppIndicator extension." },
    cli: [`curl -fsSL ${RAW}/install.sh | sh`, "npx port-process-manager"],
  },
};

function Command({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard?.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    });
  };
  return (
    <div className="command">
      <code>
        <span aria-hidden>$ </span>
        {text}
      </code>
      <button type="button" onClick={copy} aria-label={`Copy ${text}`}>
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

function Install({ os, onOs }: { os: Os; onOs: (os: Os) => void }) {
  const { app, cli } = INSTALL[os];
  return (
    <div className="install">
      <div role="tablist" aria-label="Operating system" className="tabs">
        {(Object.keys(OS_NAMES) as Os[]).map((name) => (
          <button key={name} type="button" role="tab" aria-selected={name === os} onClick={() => onOs(name)}>
            {OS_NAMES[name]}
          </button>
        ))}
      </div>
      <p className="install-label">
        Desktop app · {app.label} · <a href={RELEASES}>Releases</a>
      </p>
      {app.command && <Command text={app.command} />}
      <p className="note">{app.note}</p>
      <p className="install-label">
        CLI only · also npm, PyPI and cargo, <a href={`${REPO}#cli-only`}>see all</a>
      </p>
      {cli.map((text) => (
        <Command key={text} text={text} />
      ))}
    </div>
  );
}

const TERMINAL_COMMANDS = [
  ["ppm", "The terminal UI"],
  ["ppm list --json", "For scripts and agents"],
  ["ppm --on devbox", "Another machine"],
  ["ppm clean", "Stop what Clean up suggests"],
];

/** Each section's words. The stage beside them shows what they say. */
export function SectionCopy({ id, os, onOs, onNavigate }: { id: SectionId; os: Os; onOs: (os: Os) => void; onNavigate: (id: SectionId) => void }) {
  switch (id) {
    case "servers":
      return (
        <>
          <p className="eyebrow">
            <span className="dot" /> Free and open source · MIT
          </p>
          <h1>
            Every dev server.
            <br />
            <span className="accent">Every OS.</span>
          </h1>
          <p className="lede">
            See what's running on your ports on macOS, Windows and Linux, and on the boxes you SSH into. Which branch and agent started it, what it costs, and a
            Stop button for the ones you forgot.
          </p>
          <div className="actions">
            <a
              className="button primary"
              href="#install"
              onClick={(event) => {
                event.preventDefault();
                onNavigate("install");
              }}
            >
              Install for {OS_NAMES[os]}
            </a>
            <a className="button" href={REPO}>
              GitHub
            </a>
          </div>
          <p className="meta">No account. No telemetry. Menu bar on macOS, tray on Windows and Linux.</p>
        </>
      );
    case "machines":
      return (
        <>
          <p className="eyebrow">Machines</p>
          <h2>Every machine you work on.</h2>
          <p className="lede">
            Your laptop, a devbox over SSH, a WSL distro, a Docker container. Each gets the same list, charts and Stop button, one chip away. The first time, ppm
            asks, then installs itself over the connection.
          </p>
          <ul className="chips">
            {["ssh", "docker exec", "kubectl exec", "wsl", "your own command"].map((transport) => (
              <li key={transport}>{transport}</li>
            ))}
          </ul>
        </>
      );
    case "sessions":
      return (
        <>
          <p className="eyebrow">Sessions</p>
          <h2>Back to the session that started it.</h2>
          <p className="lede">
            Each server links to its Claude Code or Codex session, its Pane or Conductor workspace, its branch and its Vercel preview. Resume the session in your
            terminal with one click.
          </p>
        </>
      );
    case "leaks":
      return (
        <>
          <p className="eyebrow amber">Leaks</p>
          <h2>Know when a server starts leaking.</h2>
          <p className="lede">
            A server that keeps growing, or crosses your memory limit, turns amber. You get an alert with Stop and Snooze, and the chart shows how fast it grew.
          </p>
        </>
      );
    case "clean-up":
      return (
        <>
          <p className="eyebrow">Clean up</p>
          <h2>Stop what you forgot. Keep what you need.</h2>
          <p className="lede">
            Clean up lists servers whose worktree is gone, that sat idle, or that leak, and stops them together. Postgres, Redis and anything else you protect are
            never on the list.
          </p>
        </>
      );
    case "terminal":
      return (
        <>
          <p className="eyebrow">Terminal</p>
          <h2>Same servers, in your terminal.</h2>
          <p className="lede">
            <code>ppm</code> is one binary for macOS, Windows and Linux. Click the window and try it: arrow keys, Enter, Esc and q.
          </p>
          <dl className="commands">
            {TERMINAL_COMMANDS.map(([command, text]) => (
              <div key={command}>
                <dt>
                  <code>{command}</code>
                </dt>
                <dd>{text}</dd>
              </div>
            ))}
          </dl>
        </>
      );
    case "install":
      return (
        <>
          <p className="eyebrow">Install</p>
          <h2>Get it for {OS_NAMES[os]}.</h2>
          <Install os={os} onOs={onOs} />
        </>
      );
  }
}

export function Credit() {
  return (
    <p className="credit">
      Built on <a href="https://whattheport.dev">WhatThePort</a> by Tomjohn Design, under the{" "}
      <a href="https://github.com/tomjohndesign/what-the-port/blob/main/LICENSE">MIT license</a>. Made by <a href="https://greenfield.to">Greenfield</a>, the team
      behind Pane.
    </p>
  );
}
