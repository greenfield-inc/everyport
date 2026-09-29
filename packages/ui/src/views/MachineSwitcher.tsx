import type { Machine, EveryportClient } from "@everyport/protocol";
import { useState } from "react";

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

/**
 * The list area while a machine has no snapshot: connecting, failed, not
 * connected yet, or asking before it installs everyport there.
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
    machine.state === "available" && client.connectMachine
      ? { label: "Connect", onClick: run(client.connectMachine.bind(client)) }
      : machine.state === "install" && client.installEveryport
        ? { label: "Install Everyport", onClick: run(client.installEveryport.bind(client)) }
        : undefined;
  const error = failed ?? (machine.state === "install" ? machine.error : undefined);
  return (
    <div className="everyport:flex everyport:flex-col everyport:items-center everyport:gap-3 everyport:px-4 everyport:py-7 everyport:text-center everyport:text-13">
      <p className={machine.state === "error" ? "everyport:text-danger" : "everyport:text-fg2"}>{text}</p>
      {error && <p className="everyport:text-danger">{error}</p>}
      {button && (
        <button type="button" onClick={button.onClick} className="everyport:rounded-lg everyport:bg-accent everyport:px-3 everyport:py-[5px] everyport:font-medium everyport:text-fg">
          {button.label}
        </button>
      )}
    </div>
  );
}
