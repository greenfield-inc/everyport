import type { Machine, Server } from "@everyport/protocol";
import { type ReactNode, useEffect, useState } from "react";
import { memory, total } from "../format.ts";
import { AgentIcon, BranchIcon, WorkspaceIcon } from "../icons.tsx";
import { DEFAULT_ALERT_MEMORY } from "../model.ts";
import { type ThemeProps, Themed } from "../theme.tsx";

/** An agent, editor or CLI the tools step looks for. */
export type OnboardingTool = {
  name: string;
  kind: "claude_code" | "codex" | "conductor" | "pane" | "editor" | "gh";
  /** Where it was found, such as `~/.claude`. Absent when it wasn't. */
  found: string | null;
};

/** The `everyport` command in this computer's terminal. */
export type CliStatus = {
  /** Where the app installs it, such as `~/.local/bin/everyport`. */
  path: string;
  /** The installed version, or null when it isn't installed. */
  installed: string | null;
  /** The version this app installs. */
  version: string;
};

/** A machine discovery found, such as a host in `~/.ssh/config`. */
export type FoundMachine = { name: string; source: string };

/** What onboarding reads from and does on the host app. Reads never change settings. */
export type OnboardingHost = {
  platform: "macos" | "windows" | "linux";
  /** The shortcut that opens the popover, as people see it, such as ⌥⌘P. */
  shortcut: string;
  /** Null while it's being read. */
  launchAtLogin: boolean | null;
  setLaunchAtLogin: (on: boolean) => void;
  previews: boolean;
  setPreviews: (on: boolean) => void;
  tools: () => Promise<OnboardingTool[]>;
  cli: () => Promise<CliStatus>;
  /** Resolves with the new status, or rejects with a message. */
  installCli: () => Promise<CliStatus>;
  machines: () => Promise<FoundMachine[]>;
  openMachineSettings: () => void;
  /** Closes onboarding and opens the popover. */
  finish: () => void;
};

type Props = ThemeProps & {
  host: OnboardingHost;
  /** Every machine, live. The welcome and leaks steps show this computer's servers. */
  machines: Machine[];
  alertMemory?: number;
};

const STEPS = ["welcome", "leaks", "tools", "previews", "terminal", "machines", "done"] as const;
type Step = (typeof STEPS)[number];

// 5x5 dot glyphs: `#` lit, `a` lit amber, `.` unlit.
const SOCKET = [".....", ".#.#.", ".#.#.", "..#..", "....."];
const HERO: Record<Step, string[]> = {
  welcome: SOCKET,
  leaks: ["....a", "...a.", "..#..", ".#...", "#...."],
  tools: ["#....", ".#...", "..#..", ".#...", "#.###"],
  previews: [".....", "..#..", ".###.", "#####", "....."],
  terminal: [".....", ".#...", ".....", ".#.##", "....."],
  machines: ["##...", "##...", "..#..", "...##", "...##"],
  done: SOCKET,
};
/** The done step's celebration: spark, burst, fade, then settle into the socket. */
const CELEBRATION = [
  [".....", ".....", "..#..", ".....", "....."],
  [".....", "..#..", ".###.", "..#..", "....."],
  ["#.#.#", ".....", "#...#", ".....", "#.#.#"],
  ["#...#", ".....", ".....", ".....", "#...#"],
  SOCKET,
];
/** "Classic loading" from the Flicker gallery, cropped to 5x5. */
const LOADING = [
  [".###.", "....#", "....#", ".....", "....."],
  ["...#.", "....#", "....#", "....#", "...#."],
  [".....", ".....", "....#", "....#", ".###."],
  [".....", ".....", "#....", "#....", ".###."],
  [".#...", "#....", "#....", "#....", ".#..."],
  [".###.", "#....", "#....", ".....", "....."],
];
/** A checkmark drawn a dot at a time. The last frame is the finished mark. */
const CHECK = [
  [".....", ".....", "#....", ".....", "....."],
  [".....", ".....", "#....", ".#...", "....."],
  [".....", ".....", "#.#..", ".#...", "....."],
  [".....", "...#.", "#.#..", ".#...", "....."],
  ["....#", "...#.", "#.#..", ".#...", "....."],
];
const FRAME_MS = 100;

const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * First-run onboarding: a short welcome in steps, with live checks of what
 * Everyport found on this computer. Renders its own themed root.
 */
export function Onboarding({ host, machines, alertMemory = DEFAULT_ALERT_MEMORY, theme, appearance }: Props) {
  const [step, setStep] = useState<Step>("welcome");
  const index = STEPS.indexOf(step);
  const go = (offset: number) => setStep(STEPS[Math.min(STEPS.length - 1, Math.max(0, index + offset))]);
  const servers = machines.find((machine) => machine.id === "local")?.snapshot?.servers ?? [];
  const primary = step === "welcome" ? "Get started" : step === "done" ? "Open Everyport" : "Continue";
  const next = step === "done" ? host.finish : () => go(1);

  return (
    <Themed theme={theme} appearance={appearance} className="everyport-onboarding">
      <div
        className="everyport:flex everyport:h-full everyport:flex-col"
        onKeyDown={(event) => {
          if (event.key === "Enter" && !(event.target as HTMLElement).closest("button, input")) next();
        }}
      >
        <div className="everyport-scroll everyport:flex everyport:flex-1 everyport:flex-col everyport:items-center everyport:gap-5 everyport:px-8 everyport:pt-9 everyport:pb-5">
          <Hero step={step} />
          <div className="everyport:flex everyport:flex-col everyport:items-center everyport:gap-2 everyport:text-center">
            <h1 className="everyport:text-28 everyport:font-semibold everyport:text-fg">{TITLE[step]}</h1>
            <p className="everyport:text-13 everyport:leading-5 everyport:text-fg2">{message(step, host, alertMemory)}</p>
          </div>
          <div key={step} className="everyport-view everyport:flex everyport:w-full everyport:flex-col everyport:items-center everyport:gap-4">
            <StepCard step={step} host={host} servers={servers} alertMemory={alertMemory} />
          </div>
        </div>
        <div className="everyport:hairline-t everyport:flex everyport:items-center everyport:justify-between everyport:px-4 everyport:py-3.5">
          <div className="everyport:flex everyport:gap-1.5" role="img" aria-label={`Step ${index + 1} of ${STEPS.length}`}>
            {STEPS.map((item) => (
              <span
                key={item}
                className="everyport-pager-dot everyport:size-1.5 everyport:rounded-full everyport:bg-fg"
                style={{ opacity: item === step ? 1 : 0.2 }}
              />
            ))}
          </div>
          <div className="everyport:flex everyport:gap-2">
            {index > 0 && step !== "done" && <Pill onClick={() => go(-1)}>Back</Pill>}
            {step === "previews" && (
              <Pill
                onClick={() => {
                  host.setPreviews(false);
                  go(1);
                }}
              >
                Skip
              </Pill>
            )}
            <Pill primary onClick={next}>
              {primary}
            </Pill>
          </div>
        </div>
      </div>
    </Themed>
  );
}

const TITLE: Record<Step, string> = {
  welcome: "Everyport",
  leaks: "Catch leaks early",
  tools: "Your tools, in context",
  previews: "Vercel previews",
  terminal: "Everyport in your terminal",
  machines: "Your other machines",
  done: "You're set",
};

function message(step: Step, host: OnboardingHost, alertMemory: number): string {
  switch (step) {
    case "welcome":
      return "Every dev server on this computer, and on any machine you can reach. What it is, which branch it's on, and what it costs you.";
    case "leaks":
      return `Everyport warns you when a server grows past ${total(alertMemory)} or keeps growing. It sends no telemetry.`;
    case "tools":
      return "Everyport reads local session files and process info to show which agent started each server and where its code lives.";
    case "previews":
      return "See the Vercel preview for the branch each server runs. Optional.";
    case "terminal":
      return "Run everyport in any terminal to list, open and stop servers, with the same details and Clean up. Optional.";
    case "machines":
      return "Watch servers on a dev box, a container or WSL next to the ones on this computer.";
    case "done":
      return `${WHERE[host.platform]} Press ${host.shortcut} any time to open it.`;
  }
}

const WHERE: Record<OnboardingHost["platform"], string> = {
  macos: "Everyport lives in your menu bar.",
  windows: "Everyport lives in the taskbar tray. If you don't see it, click ^ next to the clock and drag the icon onto the taskbar.",
  linux: "Everyport lives in your panel. On GNOME, turn on the AppIndicator extension to see it.",
};

function StepCard({ step, host, servers, alertMemory }: { step: Step; host: OnboardingHost; servers: Server[]; alertMemory: number }) {
  switch (step) {
    case "welcome":
      return (
        <Card>
          <Row title={servers.length ? `Found ${count(servers.length, "server")} running` : "No servers running right now"}>
            <span className="everyport:font-mono everyport:text-13 everyport:text-fg2">
              {servers
                .slice(0, 3)
                .map((server) => `:${server.port}`)
                .join(" ")}
              {servers.length > 3 ? " …" : ""}
            </span>
          </Row>
        </Card>
      );
    case "leaks":
      return <Leaks servers={servers} alertMemory={alertMemory} />;
    case "tools":
      return <Tools host={host} />;
    case "previews":
      return <Previews host={host} />;
    case "terminal":
      return <Terminal host={host} />;
    case "machines":
      return <Machines host={host} />;
    case "done":
      return (
        <Card>
          <Row title="Launch at login" caption="Opens when you sign in">
            {host.launchAtLogin !== null && <Switch label="Launch at login" checked={host.launchAtLogin} onChange={host.setLaunchAtLogin} />}
          </Row>
        </Card>
      );
  }
}

/** This computer's heaviest servers, live, marked amber over the alert line or while leaking. */
function Leaks({ servers, alertMemory }: { servers: Server[]; alertMemory: number }) {
  const heaviest = [...servers].sort((a, b) => b.memory - a.memory).slice(0, 3);
  const used = servers.reduce((sum, server) => sum + server.memory, 0);
  return (
    <Card>
      <Summary title={servers.length ? `Watching ${count(servers.length, "server")}` : "Watching for servers"} detail={`${total(used)} in use`} />
      {heaviest.map((server) => {
        const warn = server.memory > alertMemory || server.clean_up?.kind === "leaking";
        return (
          <Row key={server.port} title={`:${server.port} ${server.project.name}`} divider>
            <Confirmation
              state={warn ? { kind: "warn", label: memory(server.memory) } : { kind: "ok", label: memory(server.memory) }}
              monospaced
            />
          </Row>
        );
      })}
    </Card>
  );
}

function Tools({ host }: { host: OnboardingHost }) {
  const [tools] = useCheck(host.tools);
  const found = tools?.filter((tool) => tool.found).length ?? 0;
  return (
    <Card>
      <Summary
        title={tools ? `${count(found, "tool")} found` : "Checking this computer…"}
        detail={tools ? "Check complete" : "Checking…"}
      />
      {(tools ?? PLACEHOLDER_TOOLS).map((tool) => (
        <Row key={tool.name} title={tool.name} icon={<ToolIcon kind={tool.kind} />} divider compact>
          <Confirmation
            state={!tools ? { kind: "loading", label: "Checking…" } : tool.found ? { kind: "ok", label: tool.found } : { kind: "none", label: "Not found" }}
            monospaced
          />
        </Row>
      ))}
    </Card>
  );
}

/** The rows shown while the check runs. The host's answer may leave some out, such as Conductor off macOS. */
const PLACEHOLDER_TOOLS: OnboardingTool[] = [
  { name: "Claude Code", kind: "claude_code", found: null },
  { name: "Codex", kind: "codex", found: null },
  { name: "Conductor", kind: "conductor", found: null },
  { name: "Pane", kind: "pane", found: null },
  { name: "Editor", kind: "editor", found: null },
  { name: "GitHub CLI", kind: "gh", found: null },
];

function Previews({ host }: { host: OnboardingHost }) {
  const [tools] = useCheck(host.tools);
  const gh = tools?.find((tool) => tool.kind === "gh");
  const disabled = !gh?.found;
  return (
    <Card>
      <Row
        title="Show preview links"
        caption={!tools ? "Checking for the GitHub CLI…" : gh?.found ? "Reads Vercel's GitHub deployments through gh" : "Needs the GitHub CLI (gh)"}
      >
        <Switch label="Show preview links" checked={host.previews && !disabled} disabled={disabled} onChange={host.setPreviews} />
      </Row>
    </Card>
  );
}

function Terminal({ host }: { host: OnboardingHost }) {
  const [checked, failed] = useCheck(host.cli);
  const [status, setStatus] = useState<CliStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const error = installError ?? failed;
  const cli = status ?? checked;
  const install = () => {
    setInstalling(true);
    setInstallError(null);
    host.installCli().then(
      (next) => setStatus(next),
      (reason: unknown) => setInstallError(String(reason)),
    ).finally(() => setInstalling(false));
  };
  const state: RowState = failed && !cli
    ? { kind: "none", label: "Unavailable" }
    : !cli || installing
      ? { kind: "loading", label: installing ? "Installing…" : "Checking…" }
      : cli.installed === cli.version
        ? { kind: "ok", label: cli.installed }
        : { kind: "action", label: cli.installed ? "Outdated" : "Not installed", button: cli.installed ? "Update" : "Install" };
  return (
    <>
      <Card>
        <Row title="everyport command" caption={cli ? (cli.installed ? `In ${cli.path}` : `Adds ${cli.path}`) : undefined} icon={<ToolIcon kind="terminal" />}>
          <Confirmation state={state} monospaced onAction={install} />
        </Row>
      </Card>
      {error && <p className="everyport:text-center everyport:text-11 everyport:text-warn">{error}</p>}
    </>
  );
}

function Machines({ host }: { host: OnboardingHost }) {
  const [found, failed] = useCheck(host.machines);
  return (
    <>
      <Card>
        <Summary title={failed ?? (found ? (found.length ? `Found ${count(found.length, "machine")}` : "No machines found") : "Looking for machines…")}
          detail={found || failed ? "Search complete" : "Searching…"} />
        {found?.slice(0, 3).map((machine) => (
          <Row key={machine.name} title={machine.name} divider>
            <Confirmation state={{ kind: "ok", label: machine.source }} />
          </Row>
        ))}
      </Card>
      <ul className="everyport:flex everyport:w-full everyport:flex-col everyport:gap-1 everyport:px-1 everyport:text-13 everyport:leading-[18px] everyport:text-fg2">
        <li>Everyport runs its own everyport on each machine, through a command you choose.</li>
        <li>Use ssh devbox, docker exec -i box, wsl -d Ubuntu, or a code from everyport serve.</li>
        <li>If everyport isn't there yet, the app offers to install it for you.</li>
      </ul>
      <button type="button" className="everyport:text-11 everyport:text-fg2 everyport:underline" onClick={host.openMachineSettings}>
        Add a machine in Settings
      </button>
    </>
  );
}

/** Runs a check once per mount: its answer, or why it failed. Both undefined while it runs. */
function useCheck<T>(check: () => Promise<T>): [T | undefined, string | undefined] {
  const [result, setResult] = useState<[T | undefined, string | undefined]>([undefined, undefined]);
  useEffect(() => {
    let live = true;
    check().then(
      (value) => live && setResult([value, undefined]),
      (reason: unknown) => live && setResult([undefined, String(reason)]),
    );
    return () => {
      live = false;
    };
  }, [check]);
  return result;
}

// ---------------------------------------------------------------- pieces

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;

function Hero({ step }: { step: Step }) {
  const [glyph, setGlyph] = useState(HERO[step]);
  useEffect(() => {
    if (step !== "done" || reducedMotion()) return setGlyph(HERO[step]);
    const timers = CELEBRATION.map((frame, i) => window.setTimeout(() => setGlyph(frame), 350 * i));
    return () => timers.forEach(clearTimeout);
  }, [step]);
  return (
    <div className="everyport:flex everyport:h-[120px] everyport:items-center">
      <DotGrid rows={glyph} size={110} animate />
    </div>
  );
}

/** A 5x5 dot matrix. With `animate`, each dot moves to its next state on its own, staggered by column. */
function DotGrid({ rows, size, animate = false, color = "var(--foreground)" }: { rows: string[]; size: number; animate?: boolean; color?: string }) {
  const pitch = size / 5;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden className="everyport:shrink-0">
      {rows.flatMap((line, row) =>
        [...line].map((dot, column) => (
          <circle
            key={`${row}-${column}`}
            cx={pitch * (column + 0.5)}
            cy={pitch * (row + 0.5)}
            r={pitch * 0.32}
            fill={dot === "a" ? "var(--everyport-warn)" : color}
            fillOpacity={dot === "." ? 0.12 : 1}
            style={animate ? { transition: `fill 280ms ease-in-out ${column * 40}ms, fill-opacity 280ms ease-in-out ${column * 40}ms` } : undefined}
          />
        )),
      )}
    </svg>
  );
}

type RowState =
  | { kind: "loading"; label: string }
  | { kind: "ok"; label: string }
  | { kind: "warn"; label: string }
  | { kind: "none"; label: string }
  | { kind: "action"; label: string; button: string };

const SUCCESS = "light-dark(#2f9a5d, #7fd89d)";

/** A check's result: its label (or its action button), then a mark that animates while it runs and draws a check when it passes. */
function Confirmation({ state, monospaced = false, onAction }: { state: RowState; monospaced?: boolean; onAction?: () => void }) {
  return (
    <div className="everyport:flex everyport:shrink-0 everyport:items-center everyport:gap-3">
      {state.kind === "action" ? (
        <Pill primary onClick={onAction} label={`${state.button}, ${state.label}`}>
          {state.button}
        </Pill>
      ) : (
        <span
          className={`everyport:text-right everyport:text-11 ${state.kind === "warn" ? "everyport:text-warn" : "everyport:text-fg2"} ${monospaced && state.kind !== "loading" && state.kind !== "none" ? "everyport:font-mono" : ""}`}
        >
          {state.label}
        </span>
      )}
      <Mark state={state} />
    </div>
  );
}

/** Only this 20 px slot animates: a dot spinner while checking, then a check drawn a dot at a time. */
function Mark({ state }: { state: RowState }) {
  const [frame, setFrame] = useState(() => (state.kind === "ok" ? CHECK.length - 1 : 0));
  const kind = state.kind;
  useEffect(() => {
    if (reducedMotion()) return setFrame(kind === "ok" ? CHECK.length - 1 : 0);
    setFrame(0);
    if (kind !== "loading" && kind !== "ok") return;
    const timer = window.setInterval(
      () => setFrame((f) => (kind === "loading" ? (f + 1) % LOADING.length : Math.min(f + 1, CHECK.length - 1))),
      kind === "loading" ? FRAME_MS : FRAME_MS / 2,
    );
    return () => clearInterval(timer);
  }, [kind]);
  const label = kind === "ok" ? "Confirmed" : state.label;
  return (
    <span className="everyport:flex everyport:size-5 everyport:items-center everyport:justify-center" role="img" aria-label={label}>
      {kind === "loading" ? (
        <DotGrid rows={LOADING[frame % LOADING.length]} size={20} color="var(--muted-foreground)" />
      ) : kind === "ok" ? (
        <DotGrid rows={CHECK[Math.min(frame, CHECK.length - 1)]} size={20} color={SUCCESS} />
      ) : kind === "warn" ? (
        <DotGrid rows={HERO.leaks} size={20} />
      ) : (
        <svg width="15" height="15" viewBox="0 0 15 15" fill="none" stroke="var(--muted-foreground)" strokeWidth="1.2" aria-hidden>
          <circle cx="7.5" cy="7.5" r="6.5" />
          <path d="M4.5 7.5h6" />
        </svg>
      )}
    </span>
  );
}

function Card({ children }: { children: ReactNode }) {
  return (
    <div className="everyport:flex everyport:w-full everyport:flex-col everyport:overflow-hidden everyport:rounded-[10px] everyport:bg-accent everyport:shadow-[inset_0_0_0_1px_var(--border)]">
      {children}
    </div>
  );
}

function Summary({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="everyport:flex everyport:min-h-10 everyport:items-center everyport:justify-between everyport:gap-2 everyport:p-3 everyport:text-11 everyport:text-fg2">
      <span>{title}</span>
      <span className="tabular">{detail}</span>
    </div>
  );
}

function Row({
  title,
  caption,
  icon,
  divider = false,
  compact = false,
  children,
}: {
  title: string;
  caption?: string;
  icon?: ReactNode;
  divider?: boolean;
  /** Shorter, for long lists of checks. */
  compact?: boolean;
  children: ReactNode;
}) {
  const height = compact ? "everyport:min-h-[41px] everyport:py-1.5" : caption ? "everyport:min-h-16 everyport:py-3.5" : "everyport:min-h-[49px] everyport:py-2.5";
  return (
    <div className={`everyport:flex everyport:items-center everyport:gap-3 everyport:px-3 ${height} ${divider ? "everyport:hairline-t" : ""}`}
    >
      {icon}
      <div className="everyport:flex everyport:min-w-0 everyport:flex-1 everyport:flex-col everyport:gap-1">
        <span className="everyport:clamp-1 everyport:text-13 everyport:font-medium everyport:text-fg">{title}</span>
        {caption && <span className="everyport:text-11 everyport:text-fg2">{caption}</span>}
      </div>
      {children}
    </div>
  );
}

function ToolIcon({ kind }: { kind: OnboardingTool["kind"] | "terminal" }) {
  return (
    <span className="everyport:flex everyport:size-7 everyport:shrink-0 everyport:items-center everyport:justify-center everyport:rounded-[7px] everyport:bg-panel everyport:text-fg">
      {kind === "claude_code" || kind === "codex" ? (
        <AgentIcon kind={kind} className="everyport:size-3.5" />
      ) : kind === "conductor" || kind === "pane" ? (
        <WorkspaceIcon kind={kind} className="everyport:size-3.5" />
      ) : kind === "gh" ? (
        <BranchIcon className="everyport:size-3.5" />
      ) : kind === "editor" ? (
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" aria-hidden>
          <path d="M5 4 2 7l3 3M9 4l3 3-3 3" />
        </svg>
      ) : (
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" aria-hidden>
          <path d="m2.5 4 3 3-3 3M7 10.5h4.5" />
        </svg>
      )}
    </span>
  );
}

function Pill({ primary = false, label, onClick, children }: { primary?: boolean; label?: string; onClick?: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={`everyport:rounded-full everyport:px-3.5 everyport:py-[5px] everyport:text-13 everyport:font-medium ${primary ? "everyport:bg-primary everyport:text-on-primary" : "everyport:bg-accent everyport:text-fg"}`}
    >
      {children}
    </button>
  );
}

function Switch({ label, checked, disabled = false, onChange }: { label: string; checked: boolean; disabled?: boolean; onChange: (on: boolean) => void }) {
  return (
    <input
      type="checkbox"
      role="switch"
      className="everyport-switch"
      aria-label={label}
      checked={checked}
      disabled={disabled}
      onChange={(event) => onChange(event.target.checked)}
    />
  );
}
