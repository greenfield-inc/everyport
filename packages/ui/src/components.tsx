import type { Server, ServerStatus } from "@ppm/protocol";
import { type KeyboardEvent, type MouseEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ViewContext } from "./context.ts";
import { BackIcon, LockIcon } from "./icons.tsx";
import { type Action, actionLabel, protectedNote } from "./model.ts";

/** Centred page title with an optional mark before it and a back button. */
export function Header({ title, mark, onBack }: { title: string; mark?: ReactNode; onBack?: () => void }) {
  return (
    <div className="ppm:relative ppm:flex ppm:h-5 ppm:shrink-0 ppm:items-center ppm:justify-center ppm:gap-1.5 ppm:px-7">
      {mark}
      <h1 className="ppm:clamp-1 ppm:text-13 ppm:font-medium ppm:text-fg">{title}</h1>
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

/**
 * Follows `value` over 250 ms, eased, so totals count up or down instead of
 * jumping. A new value picks up from wherever the count is.
 */
export function useTween(value: number) {
  const [shown, setShown] = useState(value);
  const current = useRef(value);
  useEffect(() => {
    const from = current.current;
    if (from === value || matchMedia("(prefers-reduced-motion: reduce)").matches) {
      current.current = value;
      setShown(value);
      return;
    }
    const start = performance.now();
    let frame = requestAnimationFrame(function step(now) {
      const t = Math.min(1, (now - start) / 250);
      current.current = from + (value - from) * (1 - (1 - t) ** 3);
      setShown(current.current);
      if (t < 1) frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [value]);
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

/**
 * "postgres is protected. Stop it anyway?" with Cancel and the action, in
 * place of a server's details. The port shows beside it; the alert's label
 * includes it.
 */
export function ProtectedConfirm({ ctx, server, action }: { ctx: ViewContext; server: Server; action: Action }) {
  const label = actionLabel[action];
  const act = (event: MouseEvent<HTMLElement>, run: () => void) => {
    event.stopPropagation();
    // The confirm unmounts; keep the keyboard on the list or panel.
    event.currentTarget.closest<HTMLElement>("[role=listbox], .ppm-panel")?.focus();
    run();
  };
  return (
    <>
      <div
        role="alert"
        aria-label={`${protectedNote([server])}. ${label} it anyway?`}
        className="ppm:flex ppm:min-w-0 ppm:flex-1 ppm:flex-col ppm:gap-px ppm:pr-2.5"
      >
        <span className="ppm:flex ppm:min-w-0 ppm:items-center ppm:gap-1.5 ppm:text-13 ppm:font-medium ppm:text-fg">
          <LockIcon className="ppm:text-warn" />
          <span className="ppm:clamp-1">{server.process_name} is protected.</span>
        </span>
        <span className="ppm:clamp-1 ppm:text-11 ppm:text-warn">{label} it anyway?</span>
      </div>
      <span className="ppm:flex ppm:shrink-0 ppm:items-center ppm:gap-1.5">
        <button type="button" onClick={(event) => act(event, ctx.cancel)} className="ppm:rounded-md ppm:bg-accent ppm:px-2.5 ppm:py-1 ppm:text-11 ppm:font-medium ppm:text-fg">
          Cancel
        </button>
        <button
          type="button"
          onClick={(event) => act(event, () => ctx.confirm(server))}
          className="ppm:rounded-md ppm:bg-danger-fill ppm:px-2.5 ppm:py-1 ppm:text-11 ppm:font-medium ppm:text-on-danger"
        >
          {label}
        </button>
      </span>
    </>
  );
}

/** A lock on a Stop button: the server is protected, so Stop asks first. */
export const ProtectedBadge = () => (
  <span className="ppm:absolute ppm:-right-1 ppm:-bottom-1 ppm:flex ppm:rounded-full ppm:bg-panel ppm:p-px ppm:text-warn">
    <LockIcon className="ppm:size-[9px]" />
  </span>
);
