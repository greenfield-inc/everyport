import type { ServerStatus } from "@ppm/protocol";
import { type KeyboardEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { BackIcon } from "./icons.tsx";

/** Centred page title with an optional back button. */
export function Header({ title, onBack }: { title: string; onBack?: () => void }) {
  return (
    <div className="ppm:relative ppm:flex ppm:h-5 ppm:shrink-0 ppm:items-center ppm:justify-center">
      <h1 className="ppm:clamp-1 ppm:px-7 ppm:text-13 ppm:font-medium ppm:text-fg">{title}</h1>
      {onBack && (
        <button
          type="button"
          aria-label="Back"
          onClick={onBack}
          className="ppm:absolute ppm:-left-1 ppm:top-0 ppm:flex ppm:size-5 ppm:items-center ppm:justify-center ppm:rounded-md ppm:text-fg2 ppm:hover:bg-accent"
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
    <span className={`ppm:flex ppm:shrink-0 ppm:flex-col ppm:justify-center ${large ? "ppm:gap-1.5" : "ppm:w-[9px] ppm:gap-[3px]"}`} aria-hidden>
      <span className="ppm:rounded-full" style={style} />
      <span className="ppm:rounded-full" style={style} />
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

  // Take focus, and give it back to the trigger on close.
  useLayoutEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    return () => trigger?.focus({ preventScroll: true });
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
      className={`ppm:absolute ppm:z-10 ppm:flex ppm:flex-col ppm:rounded-[10px] ppm:bg-[color-mix(in_oklab,var(--popover),var(--foreground)_7%)] ppm:p-[5px] ppm:shadow-[inset_0_0_0_0.5px_var(--border),0_12px_32px_rgb(0_0_0/0.5)] ${className ?? ""}`}
    >
      {title && <div className="ppm:flex ppm:items-center ppm:gap-1.5 ppm:px-2.5 ppm:pt-1.5 ppm:pb-2 ppm:text-11 ppm:text-fg2">{title}</div>}
      {items.map((item, index) =>
        item === "divider" ? (
          <div key={index} className="ppm:mx-1.5 ppm:my-1 ppm:h-[0.5px] ppm:shrink-0 ppm:bg-line" />
        ) : (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            onClick={() => {
              onClose();
              item.onSelect();
            }}
            className={`ppm:flex ppm:items-center ppm:justify-between ppm:gap-4 ppm:rounded-md ppm:px-2.5 ppm:py-[5px] ppm:text-13 ppm:hover:bg-accent ppm:focus:bg-accent ${item.danger ? "ppm:text-danger" : "ppm:text-fg"}`}
          >
            <span className="ppm:clamp-1">{item.label}</span>
            {item.hint && <span className="ppm:shrink-0 ppm:font-mono ppm:text-11 ppm:text-fg3">{item.hint}</span>}
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
