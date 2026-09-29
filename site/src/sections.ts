import type { Os } from "@everyport/protocol";

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

/**
 * The visitor's own OS. Chromium names the platform in userAgentData. Other
 * browsers have only the user agent, where Android and ChromeOS also say Linux
 * or X11, so Mac and those come first. Anything else gets macOS.
 */
export function visitorOs(): Os {
  const platform = (navigator as { userAgentData?: { platform: string } }).userAgentData?.platform;
  if (platform) return platform === "Windows" ? "windows" : platform === "Linux" ? "linux" : "macos";
  const agent = navigator.userAgent;
  if (/Windows/.test(agent)) return "windows";
  if (/Macintosh|iPhone|iPad|Android|CrOS/.test(agent)) return "macos";
  return /Linux|X11/.test(agent) ? "linux" : "macos";
}
