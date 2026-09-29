// Icons from the Paper frames. They draw in `currentColor`; callers set the color.
import type { AgentKind, CleanUpReason, WorkspaceKind } from "@everyport/protocol";

type Props = { className?: string };

const line = { fill: "none", stroke: "currentColor", strokeLinecap: "round", strokeLinejoin: "round" } as const;

export const BranchIcon = ({ className }: Props) => (
  <svg width="11" height="11" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <g {...line} strokeWidth="1.1">
      <circle cx="3" cy="2.5" r="1.4" />
      <circle cx="3" cy="9.5" r="1.4" />
      <circle cx="9" cy="4" r="1.4" />
      <path d="M3 4v4M9 5.4c0 2-2 2.3-4.8 3.4" strokeLinecap="butt" />
    </g>
  </svg>
);

export function AgentIcon({ kind, className }: Props & { kind: AgentKind }) {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
      {kind === "claude_code" ? (
        <path d="M6 1v10M1 6h10M2.5 2.5l7 7M9.5 2.5l-7 7" {...line} strokeWidth="1.4" />
      ) : (
        <g {...line} strokeWidth="1.2">
          <rect x="1" y="1.5" width="10" height="9" rx="2.5" />
          <path d="M3.5 5 5 6.2 3.5 7.4M6.2 7.6h2.3" />
        </g>
      )}
    </svg>
  );
}

export function WorkspaceIcon({ kind, className }: Props & { kind: WorkspaceKind }) {
  if (kind === "git_worktree") return <BranchIcon className={className} />;
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
      <g {...line} strokeWidth="1.1">
        {kind === "pane" ? (
          <>
            <rect x="1" y="1.5" width="10" height="9" rx="2.5" />
            <path d="M6 1.5v9" />
          </>
        ) : (
          <>
            <rect x="1.5" y="1.5" width="4" height="4" rx="1" />
            <rect x="6.5" y="1.5" width="4" height="4" rx="1" />
            <rect x="1.5" y="6.5" width="4" height="4" rx="1" />
            <rect x="6.5" y="6.5" width="4" height="4" rx="1" />
          </>
        )}
      </g>
    </svg>
  );
}

export const OpenIcon = ({ className }: Props) => (
  <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M4 2.5h5.5V8M9.5 2.5 2.5 9.5" {...line} strokeWidth="1.4" />
  </svg>
);

export const StopIcon = ({ className }: Props) => (
  <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <rect x="2.5" y="2.5" width="7" height="7" rx="1.5" fill="currentColor" />
  </svg>
);

export const BackIcon = ({ className }: Props) => (
  <svg width="14" height="14" viewBox="0 0 14 14" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M8.5 3 4.5 7l4 4" {...line} strokeWidth="1.6" />
  </svg>
);

export function Chevron({ direction, className }: Props & { direction: "down" | "up" | "right" }) {
  const d = { down: "M3 4.5 6 7.5l3-3", up: "M3 7.5 6 4.5l3 3", right: "M4.5 3 7.5 6l-3 3" }[direction];
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
      <path d={d} {...line} strokeWidth="1.4" />
    </svg>
  );
}

export const RestartIcon = ({ className }: Props) => (
  <svg width="13" height="13" viewBox="0 0 14 14" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M11.5 7a4.5 4.5 0 1 1-1.3-3.2" {...line} strokeWidth="1.4" />
    <path d="M10.6 1.6v2.6H8" {...line} strokeWidth="1.4" />
  </svg>
);

export const BroomIcon = ({ className }: Props) => (
  <svg width="14" height="14" viewBox="0 0 14 14" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M8.5 1.5 6 7M3 8.5h7l.8 4H2.2L3 8.5Z" {...line} strokeWidth="1.3" />
    <path d="M5 10.5v2M7.5 10.5v2" {...line} strokeWidth="1.1" />
  </svg>
);

export const GearIcon = ({ className }: Props) => (
  <svg width="16" height="16" viewBox="0 0 24 24" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <g {...line} strokeWidth="1.9">
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </g>
  </svg>
);

export const VercelIcon = ({ className }: Props) => (
  <svg width="11" height="10" viewBox="0 0 11 10" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M5.5 0 11 10H0z" fill="currentColor" />
  </svg>
);

export const LockIcon = ({ className }: Props) => (
  <svg width="12" height="12" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <g fill="none" stroke="currentColor" strokeWidth="1.1">
      <rect x="2.5" y="5.2" width="7" height="5" rx="1.3" />
      <path d="M4 5.2V3.8a2 2 0 0 1 4 0v1.4" />
    </g>
  </svg>
);

export const CheckIcon = ({ className }: Props) => (
  <svg width="10" height="10" viewBox="0 0 10 10" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <path d="M2 5.2 4.1 7.3 8 2.8" {...line} strokeWidth="1.6" />
  </svg>
);

export const CopyIcon = ({ className }: Props) => (
  <svg width="10" height="10" viewBox="0 0 10 10" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
    <g {...line} strokeWidth="1.1">
      <rect x="3.3" y="3.3" width="5.2" height="5.2" rx="1" />
      <path d="M6.7 1.5H2.5a1 1 0 0 0-1 1v4.2" />
    </g>
  </svg>
);

export function ReasonIcon({ kind, className }: Props & { kind: CleanUpReason["kind"] }) {
  return (
    <svg width="11" height="11" viewBox="0 0 12 12" className={`everyport:shrink-0 ${className ?? ""}`} aria-hidden>
      <g {...line} strokeWidth="1.1">
        {kind === "worktree_deleted" && (
          <>
            <path d="M1.5 3.5c0-.6.4-1 1-1h2.3l1 1.2h3.7c.6 0 1 .4 1 1V9c0 .6-.4 1-1 1h-7c-.6 0-1-.4-1-1V3.5Z" />
            <path d="M4.5 6.2h3" />
          </>
        )}
        {kind === "idle" && (
          <>
            <circle cx="6" cy="6" r="4.5" />
            <path d="M6 3.6V6l1.6 1" />
          </>
        )}
        {kind === "long_running" && (
          <>
            <rect x="1.5" y="2.5" width="9" height="8" rx="1.5" />
            <path d="M1.5 5.2h9M4 1.5v2M8 1.5v2" />
          </>
        )}
        {kind === "leaking" && <path d="M1.5 9 4.5 6l2 2 4-4.5M7.5 3.5h3v3" strokeWidth="1.2" />}
      </g>
    </svg>
  );
}

export type SocketState = "idle" | "running" | "attention";

/**
 * The socket mark (brand/socket.svg). The right slot is unlit when idle, plugs in
 * green when servers run, breathes amber on attention and unplugs when the last
 * server stops.
 */
export function Socket({ size = 18, state = "idle" }: { size?: number; state?: SocketState }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      className="everyport-socket everyport:shrink-0"
      data-state={state}
      aria-hidden
    >
      <rect x="3" y="3" width="18" height="18" rx="6" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="8" y="8" width="2.4" height="6.5" rx="1.2" />
      <rect className="everyport-socket-slot" x="13.6" y="8" width="2.4" height="6.5" rx="1.2" />
      <circle cx="12" cy="16.4" r="1.3" />
    </svg>
  );
}
