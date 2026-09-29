import type { Machine } from "@ppm/protocol";
import { useEffect, useSyncExternalStore } from "react";
import type { TauriPpmClient } from "./client";

// A plain list of every machine's servers, standing in for @ppm/ui's
// <Popover> until it lands, so the sidecar-to-page pipeline runs end to end.

type Props = {
  client: TauriPpmClient;
  initialServer?: { machineId: string; port: number };
  onReady: () => void;
};

export function ServerList({ client, onReady }: Props) {
  const machines = useSyncExternalStore(
    (listener) => client.subscribe(listener),
    () => client.machines(),
  );
  const loaded = machines.some((m) => m.snapshot);
  useEffect(() => {
    if (loaded) onReady();
  }, [loaded, onReady]);

  return (
    <div className="panel">
      {machines.map((machine) => (
        <MachineServers key={machine.id} machine={machine} client={client} />
      ))}
    </div>
  );
}

function MachineServers({ machine, client }: { machine: Machine; client: TauriPpmClient }) {
  const servers = machine.snapshot?.servers ?? [];
  return (
    <section>
      <header className="list-header">
        <span>{machine.label}</span>
        <span className="muted">
          {machine.state === "error" ? machine.error : `${servers.length} servers`}
        </span>
      </header>
      <ul role="listbox" aria-label={`Servers on ${machine.label}`}>
        {servers.map((server) => (
          <li
            key={server.port}
            role="option"
            aria-selected={false}
            className="row"
            onClick={() => void client.openUrl(machine.id, server.port)}
          >
            <span className="mono">:{server.port}</span>
            <span className="grow">
              {server.project.name}
              <span className="muted"> {server.project.branch ?? server.process_name}</span>
            </span>
            <span className="mono muted">{formatBytes(server.memory)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function formatBytes(bytes: number) {
  const mb = bytes / 1024 / 1024;
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}
