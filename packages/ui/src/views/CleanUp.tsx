import type { Server } from "@ppm/protocol";
import { Colon, Header, useTween } from "../components.tsx";
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
      <div className="ppm:flex ppm:flex-col ppm:gap-2.5 ppm:px-4 ppm:pt-3 ppm:pb-4">
        <Header title="Clean up" onBack={onBack} />
        <div className="ppm:flex ppm:flex-col ppm:gap-1.5">
          <div className="ppm:flex ppm:items-center ppm:gap-1.5">
            <span className="ppm:font-mono ppm:text-28 ppm:font-medium ppm:text-fg">{chosen.length ? `~${amount}` : "0"}</span>
            <span className="ppm:self-end ppm:pb-1 ppm:font-mono ppm:text-13 ppm:text-fg3">{unit}</span>
          </div>
          <span className="ppm:text-13 ppm:text-fg2">{chosen.length ? `can be freed by stopping ${count}` : "Pick servers to stop"}</span>
        </div>
      </div>
      <div className="ppm:hairline-t">
        {candidates.length ? (
          <div
            role="listbox"
            aria-label="Servers to stop"
            aria-multiselectable
            tabIndex={0}
            aria-activedescendant={selected === null ? undefined : `ppm-server-${selected}`}
            className="ppm-scroll ppm:flex ppm:flex-col ppm:p-1.5" style={{ maxHeight: 7 * 52 + 12 }}>
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
          <p className="ppm:px-4 ppm:py-7 ppm:text-center ppm:text-13 ppm:text-fg2">Nothing to clean up</p>
        )}
      </div>
      <div className="ppm:hairline-t ppm:flex ppm:flex-col ppm:gap-3 ppm:p-3">
        {note && (
          <div className="ppm:flex ppm:items-center ppm:gap-1.5 ppm:px-1 ppm:text-11 ppm:text-fg3">
            <LockIcon />
            {note}
          </div>
        )}
        <div className="ppm:flex ppm:items-center ppm:gap-2">
          <button type="button" onClick={onBack} className="ppm:rounded-lg ppm:bg-accent ppm:px-3.5 ppm:py-[7px] ppm:text-13 ppm:font-medium ppm:text-fg">
            Cancel
          </button>
          <button
            type="button"
            disabled={!chosen.length}
            onClick={() => {
              for (const server of chosen) ctx.stop(server);
              onBack();
            }}
            className="ppm:flex ppm:flex-1 ppm:items-center ppm:justify-center ppm:rounded-lg ppm:bg-danger-fill ppm:py-[7px] ppm:pr-2.5 ppm:pl-3 ppm:text-13 ppm:font-medium ppm:text-on-danger ppm:disabled:opacity-50"
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
      className={`ppm:flex ppm:items-center ppm:gap-3 ppm:rounded-[9px] ppm:px-2.5 ppm:py-[9px] ppm:hover:bg-accent ${selected ? "ppm:bg-accent" : ""}`}
    >
      <span
        className={`ppm:flex ppm:size-4 ppm:shrink-0 ppm:items-center ppm:justify-center ppm:rounded-[5px] ${
          checked ? "ppm:bg-primary ppm:text-on-primary" : "ppm:bg-accent ppm:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--muted-foreground)_50%,transparent)]"
        }`}
        aria-hidden
      >
        {checked && <CheckIcon />}
      </span>
      <span className="ppm:flex ppm:w-[52px] ppm:shrink-0 ppm:items-center">
        <Colon status={server.status} color={ctx.colorOf(server.port)} />
        <span className="ppm:font-mono ppm:text-13 ppm:font-medium ppm:text-fg">{server.port}</span>
      </span>
      <span className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:flex-col ppm:gap-0.5">
        <span className="ppm:clamp-1 ppm:text-13 ppm:font-medium ppm:text-fg">{server.project.name}</span>
        <span className={`ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-[5px] ppm:text-11 ${leaking ? "ppm:text-warn" : "ppm:text-fg2"}`}>
          <ReasonIcon kind={reason.kind} />
          <span className="ppm:clamp-1">{reasonText(reason, server, ctx.now)}</span>
        </span>
      </span>
      <span className={`ppm:w-[60px] ppm:shrink-0 ppm:text-right ppm:font-mono ppm:text-13 ${leaking ? "ppm:text-warn" : "ppm:text-fg/85"}`}>{memory(server.memory)}</span>
    </div>
  );
}
