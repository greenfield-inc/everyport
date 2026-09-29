import type { ServerStatus } from "@ppm/protocol";
import { type KeyboardEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { BackIcon } from "./icons.tsx";

/** Centred page title with an optional back button. */
export function Header({ title, onBack }: { title: string; onBack?: () => void }) {
  return (
    <div className="relative flex h-5 shrink-0 items-center justify-center">
      <h1 className="clamp-1 px-7 text-13 font-medium text-fg">{title}</h1>
      {onBack && (
        <button
          type="button"
          aria-label="Back"
          onClick={onBack}
          className="absolute -left-1 top-0 flex size-5 items-center justify-center rounded-md text-fg2 hover:bg-accent"
        >
          <BackIcon />
        </button>
      )}
    </div>
  );
}

/** The two status dots before every port: filled, glowing on attention, outlined when idle. */
export function Colon({ status, color, large = false }: { status: ServerStatus; color: string; large?: boolean }) {
  const size = large ? 6 : 4;
  const dotColor = status === "attention" ? "var(--ppm-warn)" : color;
  const style =
    status === "idle"
      ? { width: size, height: size, boxShadow: `inset 0 0 0 1.1px ${dotColor}` }
      : {
          width: size,
          height: size,
          background: dotColor,
          boxShadow: status === "attention" ? `0 0 5px ${dotColor}` : undefined,
        };
  return (
    <span className={`flex shrink-0 flex-col justify-center ${large ? "gap-1.5" : "w-[9px] gap-[3px]"}`} aria-hidden>
      <span className="rounded-full" style={style} />
      <span className="rounded-full" style={style} />
    </span>
  );
}

export type MenuItem =
  | { label: string; hint?: string; danger?: boolean; onSelect: () => void }
  | "divider";

/**
 * A popup menu. It takes focus, moves with the arrow keys, and closes on
 * Escape or a click outside. Handled keys call preventDefault, so the
 * popover's own shortcuts skip them.
 */
export function Menu({
  title,
  items,
  className,
  onClose,
}: {
  title?: ReactNode;
  items: MenuItem[];
  className?: string;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
  }, []);

  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [onClose]);

  const onKeyDown = (event: KeyboardEvent) => {
    const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape" || event.key === "ArrowLeft") onClose();
    else if (event.key === "ArrowDown") buttons[(index + 1) % buttons.length]?.focus();
    else if (event.key === "ArrowUp") buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
    else if (event.key !== "Enter" && event.key !== " " && event.key !== "Tab") return;
    if (event.key !== "Tab") event.preventDefault();
    event.stopPropagation();
    if (event.key === "Enter" || event.key === " ") (document.activeElement as HTMLButtonElement | null)?.click();
  };

  return (
    <div
      ref={ref}
      role="menu"
      onKeyDown={onKeyDown}
      className={`absolute z-10 flex flex-col rounded-[10px] bg-[color-mix(in_oklab,var(--popover),var(--foreground)_7%)] p-[5px] shadow-[inset_0_0_0_0.5px_var(--border),0_12px_32px_rgb(0_0_0/0.5)] ${className ?? ""}`}
    >
      {title && <div className="flex items-center gap-1.5 px-2.5 pt-1.5 pb-2 text-11 text-fg2">{title}</div>}
      {items.map((item, index) =>
        item === "divider" ? (
          <div key={index} className="mx-1.5 my-1 h-[0.5px] shrink-0 bg-line" />
        ) : (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            onClick={() => {
              onClose();
              item.onSelect();
            }}
            className={`flex items-center justify-between gap-4 rounded-md px-2.5 py-[5px] text-13 hover:bg-accent focus:bg-accent ${item.danger ? "text-danger" : "text-fg"}`}
          >
            <span className="clamp-1">{item.label}</span>
            {item.hint && <span className="shrink-0 font-mono text-11 text-fg3">{item.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}

/** Keeps removed items on screen, flagged `leaving`, for their exit animation. */
export function useLeaving<T>(items: T[], key: (item: T) => string | number, ms = 180) {
  const last = useRef(items);
  const [leaving, setLeaving] = useState<{ item: T; index: number }[]>([]);

  useEffect(() => {
    const present = new Set(items.map(key));
    const gone = last.current.flatMap((item, index) => (present.has(key(item)) ? [] : [{ item, index }]));
    last.current = items;
    setLeaving((previous) => (gone.length || previous.length ? gone : previous));
    if (!gone.length) return;
    const timer = setTimeout(() => setLeaving([]), ms);
    return () => clearTimeout(timer);
    // `key` is a pure accessor; only a new list matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, ms]);

  const shown = items.map((item) => ({ item, leaving: false }));
  for (const { item, index } of leaving) shown.splice(Math.min(index, shown.length), 0, { item, leaving: true });
  return shown;
}

/** Wraps a row so its height animates in when added and out when `leaving`. */
export function Grow({ leaving = false, children }: { leaving?: boolean; children: ReactNode }) {
  return (
    <div className="ppm-grow" data-leave={leaving || undefined} aria-hidden={leaving || undefined}>
      <div>{children}</div>
    </div>
  );
}
