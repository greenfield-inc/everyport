import type { Server } from "@ppm/protocol";
import { Colon, Header } from "../components.tsx";
import type { ViewContext } from "../context.ts";
import { memory, totalParts } from "../format.ts";
import { CheckIcon, LockIcon, ReasonIcon } from "../icons.tsx";
import { protectedNote, reasonText } from "../model.ts";

const ORDER = ["worktree_deleted", "idle", "long_running", "leaking"];
const rank = (server: Server) => ORDER.indexOf(server.clean_up?.kind ?? "");

/** Servers Clean up offers to stop, surest first. Protected servers are never offered. */
export const cleanUpCandidates = (servers: Server[]) =>
  servers.filter((server) => server.clean_up && !server.protected).sort((a, b) => rank(a) - rank(b));

type Props = {
  ctx: ViewContext;
  /** Ports that are checked. */
  checked: ReadonlySet<number>;
  /** The keyboard selection. */
  selected: number | null;
  onToggle: (port: number) => void;
  onBack: () => void;
};

export function CleanUp({ ctx, checked, selected, onToggle, onBack }: Props) {
  const candidates = cleanUpCandidates(ctx.snapshot.servers);
  const chosen = candidates.filter((server) => checked.has(server.port));
  const freed = chosen.reduce((sum, server) => sum + server.memory, 0);
  const [amount, unit] = totalParts(freed);
  const note = protectedNote(ctx.snapshot.servers.filter((server) => server.protected));
  const count = `${chosen.length} ${chosen.length === 1 ? "server" : "servers"}`;

  return (
    <>
      <div className="flex flex-col gap-2.5 px-4 pt-3 pb-4">
        <Header title="Clean up" onBack={onBack} />
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-1.5">
            <span className="font-mono text-28 font-medium text-fg">{chosen.length ? `~${amount}` : "0"}</span>
            <span className="self-end pb-1 font-mono text-13 text-fg3">{unit}</span>
          </div>
          <span className="text-13 text-fg2">{chosen.length ? `can be freed by stopping ${count}` : "Pick servers to stop"}</span>
        </div>
      </div>
      <div className="hairline-t">
        {candidates.length ? (
          <div role="listbox" aria-label="Servers to stop" aria-multiselectable className="ppm-scroll flex flex-col p-1.5" style={{ maxHeight: 7 * 52 + 12 }}>
            {candidates.map((server) => (
              <Row
                key={server.port}
                ctx={ctx}
                server={server}
                checked={checked.has(server.port)}
                selected={selected === server.port}
                onToggle={() => onToggle(server.port)}
              />
            ))}
          </div>
        ) : (
          <p className="px-4 py-7 text-center text-13 text-fg2">Nothing to clean up</p>
        )}
      </div>
      <div className="hairline-t flex flex-col gap-3 p-3">
        {note && (
          <div className="flex items-center gap-1.5 px-1 text-11 text-fg3">
            <LockIcon />
            {note}
          </div>
        )}
        <div className="flex items-center gap-2">
          <button type="button" onClick={onBack} className="rounded-lg bg-accent px-3.5 py-[7px] text-13 font-medium text-fg">
            Cancel
          </button>
          <button
            type="button"
            disabled={!chosen.length}
            onClick={() => {
              for (const server of chosen) ctx.stop(server);
              onBack();
            }}
            className="flex flex-1 items-center justify-center rounded-lg bg-danger py-[7px] pr-2.5 pl-3 text-13 font-medium text-on-danger disabled:opacity-50"
          >
            {chosen.length ? `Stop ${count} · free ${memory(freed)}` : "Stop servers"}
          </button>
        </div>
      </div>
    </>
  );
}

function Row({
  ctx,
  server,
  checked,
  selected,
  onToggle,
}: {
  ctx: ViewContext;
  server: Server;
  checked: boolean;
  selected: boolean;
  onToggle: () => void;
}) {
  const reason = server.clean_up;
  if (!reason) return null;
  const leaking = reason.kind === "leaking";
  return (
    <div
      id={`ppm-server-${server.port}`}
      data-port={server.port}
      role="option"
      aria-selected={checked}
      aria-label={`Port ${server.port}, ${server.project.name}, ${memory(server.memory)}`}
      onClick={onToggle}
      className={`flex items-center gap-3 rounded-[9px] px-2.5 py-[9px] hover:bg-accent ${selected ? "bg-accent" : ""}`}
    >
      <span
        className={`flex size-4 shrink-0 items-center justify-center rounded-[5px] ${
          checked ? "bg-primary text-on-primary" : "bg-accent shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--muted-foreground)_50%,transparent)]"
        }`}
        aria-hidden
      >
        {checked && <CheckIcon />}
      </span>
      <span className="flex w-[52px] shrink-0 items-center">
        <Colon status={server.status} color={ctx.colorOf(server.port)} />
        <span className="font-mono text-13 font-medium text-fg">{server.port}</span>
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="clamp-1 text-13 font-medium text-fg">{server.project.name}</span>
        <span className={`flex min-w-0 items-center gap-[5px] text-11 ${leaking ? "text-warn" : "text-fg2"}`}>
          <ReasonIcon kind={reason.kind} />
          <span className="clamp-1">{reasonText(reason, server, ctx.now)}</span>
        </span>
      </span>
      <span className={`w-[60px] shrink-0 text-right font-mono text-13 ${leaking ? "text-warn" : "text-fg/85"}`}>{memory(server.memory)}</span>
    </div>
  );
}
