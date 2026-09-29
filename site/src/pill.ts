import { useLayoutEffect, useRef } from "react";

const SELECTED = '[aria-checked="true"], [aria-selected="true"], [aria-current="true"]';

/**
 * A pill that slides to the selected item of a segmented control. It sets
 * --pill-x and --pill-w on the control (or on `inner` inside it), and the CSS
 * draws the pill as its ::before. Pass what changes the selection as `key`.
 */
export function usePill(key: unknown, inner?: string) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const box = inner ? ref.current?.querySelector<HTMLElement>(inner) : ref.current;
    if (!box) return;
    const place = () => {
      const item = box.querySelector<HTMLElement>(SELECTED);
      box.dataset.pill = item ? "on" : "off";
      if (!item) return;
      box.style.setProperty("--pill-x", `${item.offsetLeft}px`);
      box.style.setProperty("--pill-w", `${item.offsetWidth}px`);
    };
    place();
    // Slide only after the first placement, so the pill doesn't fly in on load.
    const frame = requestAnimationFrame(() => box.setAttribute("data-pill-ready", ""));
    const observer = new ResizeObserver(place);
    observer.observe(box);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [key, inner]);
  return ref;
}
