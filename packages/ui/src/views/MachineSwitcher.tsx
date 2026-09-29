import type { Machine, PpmClient } from "@ppm/protocol";
import { useState } from "react";

const stateLabel: Record<Machine["state"], string> = {
  available: "Not connected",
  connected: "Connected",
  connecting: "Connecting",
  install: "Needs ppm",
  installing: "Installing ppm",
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
    <div role="tablist" aria-label="Machines" className="ppm-scroll ppm:flex ppm:gap-1 ppm:overflow-x-auto ppm:px-3 ppm:pt-3">
      {machines.map((machine) => {
        const count = machine.snapshot?.servers.length;
        const dot =
          machine.state === "error"
            ? "ppm:bg-danger"
            : machine.state === "connected"
              ? machine.snapshot?.servers.some((server) => server.status === "attention")
                ? "ppm:bg-warn"
                : "ppm:bg-fg2"
              : machine.state === "available" || machine.state === "install"
                ? "ppm:ring-1 ppm:ring-fg3 ppm:ring-inset"
                : "ppm:bg-fg3 ppm:animate-pulse";
        return (
          <button
            key={machine.id}
            type="button"
            role="tab"
            aria-selected={machine.id === current}
            title={machine.error ?? `${machine.label} · ${stateLabel[machine.state]}`}
            onClick={() => onSelect(machine.id)}
            className={`ppm:flex ppm:shrink-0 ppm:items-center ppm:gap-1.5 ppm:rounded-md ppm:px-2 ppm:py-1 ppm:text-11 ${
              machine.id === current ? "ppm:bg-accent ppm:text-fg" : "ppm:text-fg2 ppm:hover:bg-accent"
            }`}
          >
            <span className={`ppm:size-1.5 ppm:rounded-full ${dot}`} aria-hidden />
            <span className="ppm:font-medium">{machine.label}</span>
            {count !== undefined && <span className="ppm:font-mono ppm:text-fg3">{count}</span>}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The list area while a machine has no snapshot: connecting, failed, not
 * connected yet, or asking before it installs ppm there.
 */
export function MachineStatus({ machine, client }: { machine: Machine; client: PpmClient }) {
  const [failed, setFailed] = useState<string>();
  const run = (action: (id: string) => Promise<void>) => () => {
    setFailed(undefined);
    action(machine.id).catch((error: unknown) => setFailed(String(error instanceof Error ? error.message : error)));
  };
  const text =
    machine.state === "available"
      ? `${machine.label} isn't connected.`
      : machine.state === "install"
        ? `${machine.error ? "" : `ppm isn't on ${machine.label} yet. `}Install ppm ${machine.install?.version ?? ""} to ${machine.install?.path ?? "~/.local/bin"}?`
        : machine.state === "installing"
          ? `Installing ppm on ${machine.label}…`
          : machine.state === "error"
            ? (machine.error ?? `Can't connect to ${machine.label}`)
            : `Connecting to ${machine.label}…`;
  const button =
    machine.state === "available" && client.connectMachine
      ? { label: "Connect", onClick: run(client.connectMachine.bind(client)) }
      : machine.state === "install" && client.installPpm
        ? { label: "Install ppm", onClick: run(client.installPpm.bind(client)) }
        : undefined;
  const error = failed ?? (machine.state === "install" ? machine.error : undefined);
  return (
    <div className="ppm:flex ppm:flex-col ppm:items-center ppm:gap-3 ppm:px-4 ppm:py-7 ppm:text-center ppm:text-13">
      <p className={machine.state === "error" ? "ppm:text-danger" : "ppm:text-fg2"}>{text}</p>
      {error && <p className="ppm:text-danger">{error}</p>}
      {button && (
        <button type="button" onClick={button.onClick} className="ppm:rounded-lg ppm:bg-accent ppm:px-3 ppm:py-[5px] ppm:font-medium ppm:text-fg">
          {button.label}
        </button>
      )}
    </div>
  );
}
