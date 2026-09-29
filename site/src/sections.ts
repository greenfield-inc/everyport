import type { Os } from "@everyport/protocol";

export const REPO = "https://github.com/greenfield-inc/everyport";
export const RELEASES = `${REPO}/releases/latest`;

/** What the popover shows while a section is on screen. */
export type View = { kind: "list" } | { kind: "detail"; port: number } | { kind: "cleanUp" } | { kind: "terminal" };

/** One entry per scroll step. `local` sections show this computer, since they point at its servers. */
export const SECTIONS = [
  { id: "servers", nav: "Servers", view: { kind: "list" }, local: false, accent: "#7fd89d" },
  { id: "machines", nav: "Machines", view: { kind: "list" }, local: false, accent: "#7cc4f0" },
  { id: "sessions", nav: "Sessions", view: { kind: "detail", port: 3000 }, local: true, accent: "#b69cf5" },
  { id: "leaks", nav: "Leaks", view: { kind: "list" }, local: true, accent: "#ffb224" },
  { id: "clean-up", nav: "Clean up", view: { kind: "cleanUp" }, local: true, accent: "#6fd6c4" },
  { id: "terminal", nav: "Terminal", view: { kind: "terminal" }, local: false, accent: "#9fd87f" },
  { id: "install", nav: "Install", view: { kind: "list" }, local: false, accent: "#7fd89d" },
] as const satisfies readonly { id: string; nav: string; view: View; local: boolean; accent: string }[];

export type SectionId = (typeof SECTIONS)[number]["id"];

export const OS_NAMES: Record<Os, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };

/** The visitor's own OS, from the user agent. */
export function visitorOs(): Os {
  const agent = navigator.userAgent;
  if (/Windows/.test(agent)) return "windows";
  if (/Linux|X11|CrOS|Android/.test(agent)) return "linux";
  return "macos";
}
