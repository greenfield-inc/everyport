import type { Machine } from "@ppm/protocol";

const stateLabel: Record<Machine["state"], string> = {
  connected: "Connected",
  connecting: "Connecting",
  installing: "Installing ppm",
  error: "Can't connect",
};

/** One chip per machine, shown when more than one is connected. */
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
    <div role="tablist" aria-label="Machines" className="ppm-scroll flex gap-1 overflow-x-auto px-3 pt-3">
      {machines.map((machine) => {
        const count = machine.snapshot?.servers.length;
        const dot =
          machine.state === "error"
            ? "bg-danger"
            : machine.state === "connected"
              ? machine.snapshot?.servers.some((server) => server.status === "attention")
                ? "bg-warn"
                : "bg-fg2"
              : "bg-fg3 animate-pulse";
        return (
          <button
            key={machine.id}
            type="button"
            role="tab"
            aria-selected={machine.id === current}
            title={machine.error ?? `${machine.label} · ${stateLabel[machine.state]}`}
            onClick={() => onSelect(machine.id)}
            className={`flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1 text-11 ${
              machine.id === current ? "bg-accent text-fg" : "text-fg2 hover:bg-accent"
            }`}
          >
            <span className={`size-1.5 rounded-full ${dot}`} aria-hidden />
            <span className="font-medium">{machine.label}</span>
            {count !== undefined && <span className="font-mono text-fg3">{count}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** The list area while a machine has no snapshot yet, or has failed. */
export function MachineStatus({ machine }: { machine: Machine }) {
  const text =
    machine.state === "error"
      ? (machine.error ?? `Can't connect to ${machine.label}`)
      : machine.state === "installing"
        ? `Installing ppm on ${machine.label}…`
        : `Connecting to ${machine.label}…`;
  return <p className={`px-4 py-7 text-center text-13 ${machine.state === "error" ? "text-danger" : "text-fg2"}`}>{text}</p>;
}
