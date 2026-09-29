import { LOCAL_MACHINE, type CheckStep, type Machine, type EveryportClient } from "@everyport/protocol";
import { Fragment, useEffect, useState } from "react";
import { copy } from "../context.ts";
import { CheckIcon, CopyIcon } from "../icons.tsx";

const stateLabel: Record<Machine["state"], string> = {
  available: "Not connected",
  connected: "Connected",
  connecting: "Connecting",
  install: "Needs Everyport",
  installing: "Installing Everyport",
  error: "Can't connect",
};

/** One chip per machine, shown when the client knows more than one. */
export function MachineSwitcher({
  machines,
  current,
  onSelect,
}: {
  machines: Machine[];
  current: string;
  onSelect: (id: string) => void;
}) {
  return (
    <div role="tablist" aria-label="Machines" className="everyport-scroll everyport:flex everyport:gap-1 everyport:overflow-x-auto everyport:px-3 everyport:pt-3">
      {machines.map((machine) => {
        const count = machine.snapshot?.servers.length;
        const dot =
          machine.state === "error"
            ? "everyport:bg-danger"
            : machine.state === "connected"
              ? machine.snapshot?.servers.some((server) => server.status === "attention")
                ? "everyport:bg-warn"
                : "everyport:bg-fg2"
              : machine.state === "available" || machine.state === "install"
                ? "everyport:ring-1 everyport:ring-fg3 everyport:ring-inset"
                : "everyport:bg-fg3 everyport:animate-pulse";
        return (
          <button
            key={machine.id}
            type="button"
            role="tab"
            aria-selected={machine.id === current}
            title={machine.error ?? `${machine.label} · ${stateLabel[machine.state]}`}
            onClick={() => onSelect(machine.id)}
            className={`everyport:flex everyport:shrink-0 everyport:items-center everyport:gap-1.5 everyport:rounded-md everyport:px-2 everyport:py-1 everyport:text-11 ${
              machine.id === current ? "everyport:bg-accent everyport:text-fg" : "everyport:text-fg2 everyport:hover:bg-accent"
            }`}
          >
            <span className={`everyport:size-1.5 everyport:rounded-full ${dot}`} aria-hidden />
            <span className="everyport:font-medium">{machine.label}</span>
            {count !== undefined && <span className="everyport:font-mono everyport:text-fg3">{count}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** A command the user copies with a click. */
function CopyCode({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);
  const onCopy = () => {
    copy(code);
    setCopied(true);
  };
  // Inline code rather than a button, so a long command wraps with the sentence.
  return (
    <code
      role="button"
      tabIndex={0}
      title="Copy"
      onClick={onCopy}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        onCopy();
      }}
      className="everyport:cursor-pointer everyport:rounded everyport:bg-accent everyport:px-1 everyport:font-mono everyport:text-11 everyport:text-fg everyport:[box-decoration-break:clone] everyport:hover:bg-accent/70"
    >
      {code}
      <span className="everyport:ml-1 everyport:inline-flex everyport:items-center everyport:gap-0.5 everyport:align-middle everyport:font-sans everyport:text-fg3">
        {copied ? (
          <>
            <CheckIcon /> Copied
          </>
        ) : (
          <CopyIcon />
        )}
      </span>
    </code>
  );
}

/** Text with each `command` as code the user copies with a click. */
function WithCode({ text }: { text: string }) {
  return (
    <>{text.split("`").map((part, i) => (i % 2 ? <CopyCode key={i} code={part} /> : <Fragment key={i}>{part}</Fragment>))}</>
  );
}

/** The steps to reach a machine, with the fix under the one that failed. */
export function Checklist({ steps }: { steps: CheckStep[] }) {
  const [details, setDetails] = useState(false);
  return (
    <ol aria-label="Connection check" className="everyport:flex everyport:w-full everyport:flex-col everyport:gap-1.5 everyport:text-left everyport:text-13">
      {steps.map((step, i) => (
        <li key={i} className="everyport:flex everyport:gap-2">
          <span aria-label={step.ok ? "passed" : "failed"} className={`everyport:w-3 everyport:shrink-0 everyport:text-center ${step.ok ? "everyport:text-fg3" : "everyport:text-danger"}`}>
            {step.ok ? "✓" : "✗"}
          </span>
          <div className="everyport:flex everyport:min-w-0 everyport:flex-col everyport:gap-1">
            <span className={step.ok ? "everyport:text-fg2" : "everyport:font-medium everyport:text-fg"}>{step.label}</span>
            {step.fix && (
              <p className="everyport:text-fg2">
                <WithCode text={step.fix} />
              </p>
            )}
            {step.detail && (
              <>
                <button type="button" onClick={() => setDetails(!details)} className="everyport:self-start everyport:text-11 everyport:text-fg3 everyport:hover:text-fg2">
                  {details ? "Hide details" : "Details"}
                </button>
                {details && <pre className="everyport:select-text everyport:whitespace-pre-wrap everyport:break-words everyport:font-mono everyport:text-11 everyport:text-fg3">{step.detail}</pre>}
              </>
            )}
          </div>
        </li>
      ))}
    </ol>
  );
}

/**
 * The list area while a machine has no snapshot: connecting, failed, not
 * connected yet, or asking before it installs everyport there. A failed
 * machine shows the steps to reach it and the fix for the one that failed.
 */
export function MachineStatus({ machine, client }: { machine: Machine; client: EveryportClient }) {
  const [failed, setFailed] = useState<string>();
  const run = (action: (id: string) => Promise<void>) => () => {
    setFailed(undefined);
    action(machine.id).catch((error: unknown) => setFailed(String(error instanceof Error ? error.message : error)));
  };
  const text =
    machine.state === "available"
      ? `${machine.label} isn't connected.`
      : machine.state === "install"
        ? `${machine.error ? "" : `Everyport isn't on ${machine.label} yet. `}Install Everyport ${machine.install?.version ?? ""} to ${machine.install?.path ?? "~/.local/bin"}?`
        : machine.state === "installing"
          ? `Installing Everyport on ${machine.label}…`
          : machine.state === "error"
            ? (machine.error ?? `Can't connect to ${machine.label}`)
            : `Connecting to ${machine.label}…`;
  const button =
    (machine.state === "available" || (machine.state === "error" && machine.id !== LOCAL_MACHINE)) && client.connectMachine
      ? { label: machine.state === "error" ? "Check again" : "Connect", onClick: run(client.connectMachine.bind(client)) }
      : machine.state === "install" && client.installEveryport
        ? { label: "Install Everyport", onClick: run(client.installEveryport.bind(client)) }
        : undefined;
  const error = failed ?? (machine.state === "install" ? machine.error : undefined);
  return (
    <div className="everyport:flex everyport:flex-col everyport:items-center everyport:gap-3 everyport:px-4 everyport:py-7 everyport:text-center everyport:text-13">
      {machine.state === "error" && machine.check ? (
        <>
          <p className="everyport:text-fg">Can't connect to {machine.label}</p>
          <Checklist steps={machine.check} />
        </>
      ) : (
        <p className={machine.state === "error" ? "everyport:text-danger" : "everyport:text-fg2"}>{text}</p>
      )}
      {error && <p className="everyport:text-danger">{error}</p>}
      {button && (
        <button type="button" onClick={button.onClick} className="everyport:rounded-lg everyport:bg-accent everyport:px-3 everyport:py-[5px] everyport:font-medium everyport:text-fg">
          {button.label}
        </button>
      )}
    </div>
  );
}
