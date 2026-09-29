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

/** The list area while a machine has no snapshot yet, or has failed. */
export function MachineStatus({ machine }: { machine: Machine }) {
  const text =
    machine.state === "error"
      ? (machine.error ?? `Can't connect to ${machine.label}`)
      : machine.state === "installing"
        ? `Installing ppm on ${machine.label}…`
        : `Connecting to ${machine.label}…`;
  return <p className={`ppm:px-4 ppm:py-7 ppm:text-center ppm:text-13 ${machine.state === "error" ? "ppm:text-danger" : "ppm:text-fg2"}`}>{text}</p>;
}
