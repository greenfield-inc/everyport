import type { Os } from "@everyport/protocol";
import { useEffect, useRef, useState } from "react";
import { usePill } from "./pill.ts";
import { OS_NAMES, RELEASES, REPO, type SectionId } from "./sections.ts";

const RAW = "https://github.com/greenfield-inc/everyport/releases/latest/download";

/** The one-line app install, served from the site root. */
const APP_COMMAND: Record<Os, string> = {
  macos: `curl -fsSL ${__SITE_URL__}install.sh | sh`,
  windows: `irm ${__SITE_URL__}install.ps1 | iex`,
  linux: `curl -fsSL ${__SITE_URL__}install.sh | sh`,
};

/** CLI-only installs from the README. */
const CLI_COMMANDS: Record<Os, string[]> = {
  macos: [`curl -fsSL ${RAW}/install.sh | sh`, "brew install greenfield-inc/tap/everyport"],
  windows: [`irm ${RAW}/install.ps1 | iex`, "npx everyport"],
  linux: [`curl -fsSL ${RAW}/install.sh | sh`, "npx everyport"],
};

type Download = { id: string; os: Os; label: string; detail: string; arch: "aarch64" | "x86_64"; ext: string };

/** Release files, as the release workflow names them. */
const DOWNLOADS: Download[] = [
  { id: "mac-arm", os: "macos", label: "macOS", detail: "Apple Silicon", arch: "aarch64", ext: "dmg" },
  { id: "mac-intel", os: "macos", label: "macOS", detail: "Intel", arch: "x86_64", ext: "dmg" },
  { id: "windows", os: "windows", label: "Windows", detail: ".msi", arch: "x86_64", ext: "msi" },
  { id: "appimage", os: "linux", label: "Linux", detail: ".AppImage", arch: "x86_64", ext: "AppImage" },
  { id: "deb", os: "linux", label: "Linux", detail: ".deb", arch: "x86_64", ext: "deb" },
  { id: "rpm", os: "linux", label: "Linux", detail: ".rpm", arch: "x86_64", ext: "rpm" },
];

const href = ({ arch, ext }: Download) => `${RAW}/everyport-${__EVERYPORT_VERSION__}-${arch}.${ext}`;

/**
 * Apple Silicon unless the browser says Intel. Chromium tells through
 * userAgentData; Safari and Firefox report every Mac as Intel, so they get the default.
 */
function useIntelMac() {
  const [intel, setIntel] = useState(false);
  useEffect(() => {
    const data = (navigator as { userAgentData?: { getHighEntropyValues(hints: string[]): Promise<{ architecture?: string }> } }).userAgentData;
    data
      ?.getHighEntropyValues(["architecture"])
      .then((values) => setIntel(values.architecture === "x86"))
      .catch(() => {});
  }, []);
  return intel;
}

const CopyIcon = () => (
  <svg className="copy-icon" width="14" height="14" viewBox="0 0 14 14" aria-hidden>
    <g className="icon-copy">
      <rect x="4.5" y="4.5" width="8" height="8" rx="1.8" fill="none" stroke="currentColor" strokeWidth="1.3" />
      <path d="M9.5 2.8V2.6A1.6 1.6 0 0 0 7.9 1H3A2 2 0 0 0 1 3v4.9a1.6 1.6 0 0 0 1.6 1.6h.2" fill="none" stroke="currentColor" strokeWidth="1.3" />
    </g>
    <path className="icon-check" d="M2.5 7.4 5.6 10.4 11.5 3.8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

function Command({ text, primary = false }: { text: string; primary?: boolean }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const copy = () => {
    void navigator.clipboard?.writeText(text).then(() => {
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <div className="command" data-primary={primary || undefined}>
      <code>
        <span aria-hidden>$ </span>
        {text}
      </code>
      <button type="button" onClick={copy} data-copied={copied || undefined} aria-label={copied ? "Copied" : `Copy ${text}`}>
        <CopyIcon />
        <span aria-live="polite">{copied ? "Copied" : "Copy"}</span>
      </button>
    </div>
  );
}

function Install({ os, onOs }: { os: Os; onOs: (os: Os) => void }) {
  const intel = useIntelMac();
  const primary = os === "macos" ? [intel ? "mac-intel" : "mac-arm"] : os === "windows" ? ["windows"] : ["appimage", "deb"];
  const files = DOWNLOADS.filter((file) => primary.includes(file.id));
  const others = DOWNLOADS.filter((file) => !primary.includes(file.id));
  return (
    <div className="install">
      <div ref={usePill(os)} role="tablist" aria-label="Operating system" className="tabs">
        {(Object.keys(OS_NAMES) as Os[]).map((name) => (
          <button key={name} type="button" role="tab" aria-selected={name === os} onClick={() => onOs(name)}>
            {OS_NAMES[name]}
          </button>
        ))}
      </div>
      <Command text={APP_COMMAND[os]} primary />
      <div className="downloads">
        {files.map((file) => (
          <a key={file.id} className="button download" href={href(file)}>
            <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
              <path d="M7 1.5v8M3.5 6.5 7 10l3.5-3.5M2 12.5h10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            Download .{file.ext}
            {file.os === "macos" && <span className="download-detail">{file.detail}</span>}
          </a>
        ))}
      </div>
      <p className="note">The command downloads the app, installs it and opens it, without the unsigned-app prompt a browser download shows.</p>
      <p className="others">
        <span>Other platforms</span>
        {others.map((file) => (
          <a key={file.id} href={href(file)}>
            {file.label} {file.detail}
          </a>
        ))}
        <a href={RELEASES}>All releases</a>
      </p>
      <p className="install-label">
        CLI only · also npm, PyPI and cargo, <a href={`${REPO}#cli-only`}>see all</a>
      </p>
      {CLI_COMMANDS[os].map((text) => (
        <Command key={text} text={text} />
      ))}
    </div>
  );
}

const TERMINAL_COMMANDS = [
  ["everyport", "The terminal UI"],
  ["everyport list --json", "For scripts and agents"],
  ["everyport --on devbox", "Another machine"],
  ["everyport clean", "Stop what Clean up suggests"],
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
            Your laptop, a devbox over SSH, a WSL distro, a Docker container. Each gets the same list, charts and Stop button, one chip away. The first time, everyport
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
            <code>everyport</code> is one binary for macOS, Windows and Linux. Click the window and try it: arrow keys, Enter, Esc and q.
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
      <a href="https://github.com/tomjohndesign/what-the-port/blob/main/LICENSE">MIT license</a>. Made by Dcouple, Inc., the team
      behind Pane.
    </p>
  );
}
