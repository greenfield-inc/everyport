/**
 * The mark: a port drawn as a wall outlet. The right slot is lit with
 * `accent`; `state` makes it breathe amber (a leak) or go dim (no servers).
 */
export function Socket({ size = 16, accent = "currentColor", state = "on" }: { size?: number; accent?: string; state?: "on" | "alert" | "off" }) {
  return (
    <svg className="socket" data-state={state} width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <rect x="3" y="3" width="18" height="18" rx="6" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="8" y="8" width="2.4" height="6.5" rx="1.2" fill="currentColor" />
      <rect className="slot" x="13.6" y="8" width="2.4" height="6.5" rx="1.2" fill={accent} />
      <circle cx="12" cy="16.4" r="1.3" fill="currentColor" />
    </svg>
  );
}
