import type { Server } from "@everyport/protocol";
import { Header, StatusSlot, useTween } from "../components.tsx";
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
  const [amount, unit] = totalParts(useTween(freed));
  const note = protectedNote(ctx.snapshot.servers.filter((server) => server.protected));
  const count = `${chosen.length} ${chosen.length === 1 ? "server" : "servers"}`;

  return (
    <>
      <div className="everyport:flex everyport:flex-col everyport:gap-2.5 everyport:px-4 everyport:pt-3 everyport:pb-4">
        <Header title="Clean up" onBack={onBack} />
        <div className="everyport:flex everyport:flex-col everyport:gap-1.5">
          <div className="everyport:flex everyport:items-center everyport:gap-1.5">
            <span className="everyport:font-mono everyport:text-28 everyport:font-medium everyport:text-fg">{chosen.length ? `~${amount}` : "0"}</span>
            <span className="everyport:self-end everyport:pb-1 everyport:font-mono everyport:text-13 everyport:text-fg3">{unit}</span>
          </div>
          <span className="everyport:text-13 everyport:text-fg2">{chosen.length ? `can be freed by stopping ${count}` : "Pick servers to stop"}</span>
        </div>
      </div>
      <div className="everyport:hairline-t">
        {candidates.length ? (
          <div
            role="listbox"
            aria-label="Servers to stop"
            aria-multiselectable
            tabIndex={0}
            aria-activedescendant={selected === null ? undefined : `everyport-server-${selected}`}
            className="everyport-scroll everyport:flex everyport:flex-col everyport:p-1.5" style={{ maxHeight: 7 * 52 + 12 }}>
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
          <p className="everyport:px-4 everyport:py-7 everyport:text-center everyport:text-13 everyport:text-fg2">Nothing to clean up</p>
        )}
      </div>
      <div className="everyport:hairline-t everyport:flex everyport:flex-col everyport:gap-3 everyport:p-3">
        {note && (
          <div className="everyport:flex everyport:items-center everyport:gap-1.5 everyport:px-1 everyport:text-11 everyport:text-fg3">
            <LockIcon />
            {note}
          </div>
        )}
        <div className="everyport:flex everyport:items-center everyport:gap-2">
          <button type="button" onClick={onBack} className="everyport:rounded-lg everyport:bg-accent everyport:px-3.5 everyport:py-[7px] everyport:text-13 everyport:font-medium everyport:text-fg">
            Cancel
          </button>
          <button
            type="button"
            disabled={!chosen.length}
            onClick={() => {
              for (const server of chosen) ctx.stop(server);
              onBack();
            }}
            className="everyport:flex everyport:flex-1 everyport:items-center everyport:justify-center everyport:rounded-lg everyport:bg-danger-fill everyport:py-[7px] everyport:pr-2.5 everyport:pl-3 everyport:text-13 everyport:font-medium everyport:text-on-danger everyport:disabled:opacity-50"
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
      id={`everyport-server-${server.port}`}
      data-port={server.port}
      role="option"
      aria-selected={checked}
      aria-label={`Port ${server.port}, ${server.project.name}, ${memory(server.memory)}`}
      onClick={onToggle}
      className={`everyport:flex everyport:items-center everyport:gap-3 everyport:rounded-[9px] everyport:px-2.5 everyport:py-[9px] everyport:hover:bg-accent ${selected ? "everyport:bg-accent" : ""}`}
    >
      <span
        className={`everyport:flex everyport:size-4 everyport:shrink-0 everyport:items-center everyport:justify-center everyport:rounded-[5px] ${
          checked ? "everyport:bg-primary everyport:text-on-primary" : "everyport:bg-accent everyport:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--muted-foreground)_50%,transparent)]"
        }`}
        aria-hidden
      >
        {checked && <CheckIcon />}
      </span>
      <span className="everyport:flex everyport:w-[52px] everyport:shrink-0 everyport:items-center">
        <StatusSlot status={server.status} color={ctx.colorOf(server.port)} />
        <span className="everyport:font-mono everyport:text-13 everyport:font-medium everyport:text-fg">{server.port}</span>
      </span>
      <span className="everyport:flex everyport:min-w-0 everyport:flex-1 everyport:flex-col everyport:gap-0.5">
        <span className="everyport:clamp-1 everyport:text-13 everyport:font-medium everyport:text-fg">{server.project.name}</span>
        <span className={`everyport:flex everyport:min-w-0 everyport:items-center everyport:gap-[5px] everyport:text-11 ${leaking ? "everyport:text-warn" : "everyport:text-fg2"}`}>
          <ReasonIcon kind={reason.kind} />
          <span className="everyport:clamp-1">{reasonText(reason, server, ctx.now)}</span>
        </span>
      </span>
      <span className={`everyport:w-[60px] everyport:shrink-0 everyport:text-right everyport:font-mono everyport:text-13 ${leaking ? "everyport:text-warn" : "everyport:text-fg/85"}`}>{memory(server.memory)}</span>
    </div>
  );
}
