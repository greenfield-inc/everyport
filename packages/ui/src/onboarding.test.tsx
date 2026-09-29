// @vitest-environment jsdom
// Onboarding's step flow through a rendered <Onboarding>, with a fake host.
import { fixtureSnapshot, type Machine } from "@everyport/protocol";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Onboarding, type OnboardingHost } from "./index.ts";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
window.matchMedia = ((query: string) => ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} })) as never;

const local: Machine = { id: "local", label: "This Mac", host: null, state: "connected", snapshot: fixtureSnapshot };

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  document.body.replaceChildren();
});

function render(platform: OnboardingHost["platform"] = "macos") {
  const host = {
    platform,
    shortcut: "Ctrl+Alt+P",
    launchAtLogin: true,
    setLaunchAtLogin: vi.fn(),
    previews: true,
    setPreviews: vi.fn(),
    tools: vi.fn(async () => [{ name: "GitHub CLI", kind: "gh" as const, found: "gh" }]),
    cli: vi.fn(async () => ({ path: "~/.local/bin/everyport", installed: false, hint: null })),
    installCli: vi.fn(async () => ({
      path: "~/.local/bin/everyport",
      installed: true,
      hint: "If your terminal can't find it, add ~/.local/bin to your PATH.",
    })),
    machines: vi.fn(async () => [{ name: "devbox", source: "SSH config" }]),
    openMachineSettings: vi.fn(),
    finish: vi.fn(),
  } satisfies OnboardingHost;
  const element = document.createElement("div");
  document.body.append(element);
  root = createRoot(element);
  act(() => root!.render(<Onboarding host={host} machines={[local]} />));
  const click = async (label: string) => {
    const button = [...element.querySelectorAll("button")].find((b) => b.textContent === label);
    if (!button) throw new Error(`no ${label} button in: ${element.textContent}`);
    await act(async () => button.click());
  };
  const title = () => element.querySelector("h1")?.textContent;
  return { host, element, click, title };
}

describe("onboarding", () => {
  it("walks welcome, leaks, tools, previews, terminal, machines and done, then opens Everyport", async () => {
    const { host, click, title } = render();
    expect(title()).toBe("Everyport");
    await click("Get started");
    expect(title()).toBe("Catch leaks early");
    await click("Continue");
    expect(title()).toBe("Your tools, in context");
    await click("Back");
    expect(title()).toBe("Catch leaks early");
    await click("Continue");
    await click("Continue");
    expect(title()).toBe("Vercel previews");
    await click("Continue");
    expect(title()).toBe("Everyport in your terminal");
    await click("Continue");
    expect(title()).toBe("Your other machines");
    await click("Continue");
    expect(title()).toBe("You're set");
    expect(host.finish).not.toHaveBeenCalled();
    await click("Open Everyport");
    expect(host.finish).toHaveBeenCalledOnce();
  });

  it("turns previews off when they're skipped", async () => {
    const { host, click, title } = render();
    for (const label of ["Get started", "Continue", "Continue"]) await click(label);
    await click("Skip");
    expect(host.setPreviews).toHaveBeenCalledWith(false);
    expect(title()).toBe("Everyport in your terminal");
  });

  it("installs the everyport command on request", async () => {
    const { host, element, click } = render();
    for (const label of ["Get started", "Continue", "Continue", "Continue"]) await click(label);
    expect(element.textContent).toContain("Adds ~/.local/bin/everyport");
    await click("Install");
    expect(host.installCli).toHaveBeenCalledOnce();
    expect(element.textContent).toContain("In ~/.local/bin/everyport");
    expect(element.textContent).toContain("add ~/.local/bin to your PATH");
  });

  it("says where the tray icon lives on Windows", async () => {
    const { element, click } = render("windows");
    for (let i = 0; i < 6; i++) await click(i === 0 ? "Get started" : "Continue");
    expect(element.textContent).toContain("If you don't see it, click ^ next to the clock and drag the icon onto the taskbar.");
    expect(element.querySelector<HTMLInputElement>("[role=switch][aria-label='Launch at login']")?.checked).toBe(true);
  });
});
