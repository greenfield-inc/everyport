import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

/** Sizes the native window to the element's rendered size. */
export function useFitWindow<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      const { width, height } = element.getBoundingClientRect();
      if (height > 0) void invoke("fit_window", { width: Math.ceil(width), height: Math.ceil(height) });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return ref;
}
