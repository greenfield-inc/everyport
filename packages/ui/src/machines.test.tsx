// @vitest-environment jsdom
// Other machines through a rendered <Popover>: the README's "The app offers
// to install everyport on a machine the first time you connect", and discovered
// hosts that connect once picked.
import { fixtureSnapshot, type Machine, type EveryportClient } from "@everyport/protocol";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Popover } from "./index.ts";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
window.matchMedia = ((query: string) => ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} })) as never;

const local: Machine = { id: "local", label: "This Mac", host: null, state: "connected", snapshot: fixtureSnapshot };

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  document.body.replaceChildren();
});

function render(remote: Machine) {
  const client = {
    machines: () => [local, remote],
    subscribe: () => () => {},
    call: vi.fn(async () => {}),
    openUrl: vi.fn(async () => {}),
    connectMachine: vi.fn(async () => {}),
    installEveryport: vi.fn(async () => {}),
  } satisfies EveryportClient;
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root!.render(<Popover client={client} initialServer={{ machineId: remote.id, port: 0 }} />));
  const button = (label: string) => [...host.querySelectorAll("button")].find((b) => b.textContent === label);
  return { client, host, button };
}

describe("another machine", () => {
  it("asks before installing everyport, and installs on yes", () => {
    const { client, host, button } = render({
      id: "devbox",
      label: "devbox",
      host: null,
      state: "install",
      install: { version: "0.1.0", path: "/home/me/.local/bin/everyport" },
      snapshot: null,
    });
    expect(host.textContent).toContain("Everyport isn't on devbox yet. Install Everyport 0.1.0 to /home/me/.local/bin/everyport?");
    expect(client.installEveryport).not.toHaveBeenCalled();
    act(() => button("Install Everyport")!.click());
    expect(client.installEveryport).toHaveBeenCalledWith("devbox");
  });

  it("gives the reason instead, when an install failed or everyport there is incompatible", () => {
    const { host, button } = render({
      id: "devbox",
      label: "devbox",
      host: null,
      state: "install",
      error: "Couldn't install everyport: no space left on device",
      install: { version: "0.1.0", path: "/home/me/.local/bin/everyport" },
      snapshot: null,
    });
    expect(host.textContent).toContain("Install Everyport 0.1.0 to /home/me/.local/bin/everyport?");
    expect(host.textContent).not.toContain("isn't on devbox yet");
    expect(host.textContent).toContain("Couldn't install everyport: no space left on device");
    expect(button("Install Everyport")).toBeDefined();
  });

  it("connects a discovered host when picked", () => {
    const { client, host, button } = render({ id: "mini", label: "mini", host: null, state: "available", snapshot: null });
    expect(host.textContent).toContain("mini isn't connected.");
    act(() => button("Connect")!.click());
    expect(client.connectMachine).toHaveBeenCalledWith("mini");
  });

  it("shows the connection check with the fix, and checks again", () => {
    const { client, host, button } = render({
      id: "mini",
      label: "Studio Mac",
      host: null,
      state: "error",
      check: [
        { label: "mini.tail1234.ts.net resolves to 100.64.0.7", ok: true },
        {
          label: "Nothing listens on port 22",
          ok: false,
          fix: "SSH is off on Studio Mac. On Studio Mac, turn on System Settings > General > Sharing > Remote Login.",
          detail: "Connection refused (os error 61)",
        },
      ],
      snapshot: null,
    });
    const steps = [...host.querySelectorAll("li")].map((li) => li.textContent);
    expect(steps[0]).toBe("✓mini.tail1234.ts.net resolves to 100.64.0.7");
    expect(steps[1]).toContain("✗Nothing listens on port 22SSH is off on Studio Mac.");
    expect(host.textContent).not.toContain("Connection refused");
    act(() => button("Details")!.click());
    expect(host.textContent).toContain("Connection refused (os error 61)");
    act(() => button("Check again")!.click());
    expect(client.connectMachine).toHaveBeenCalledWith("mini");
  });

  it("copies a command in the fix with a click", () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { host } = render({
      id: "box",
      label: "box",
      host: null,
      state: "error",
      check: [{ label: "Nothing listens on port 22", ok: false, fix: "SSH is off on box. On box, run `sudo systemctl enable --now ssh`." }],
      snapshot: null,
    });
    const code = [...host.querySelectorAll('[role="button"]')].find((b) => b.textContent?.startsWith("sudo systemctl")) as HTMLElement;
    act(() => code.click());
    expect(writeText).toHaveBeenCalledWith("sudo systemctl enable --now ssh");
    expect(code.textContent).toBe("sudo systemctl enable --now ssh Copied");
  });

  it("tries a failed machine again when its chip is picked", () => {
    const { client, host } = render({ id: "mini", label: "mini", host: null, state: "error", error: "timed out", snapshot: null });
    const chip = [...host.querySelectorAll('[role="tab"]')].find((tab) => tab.textContent === "mini") as HTMLElement;
    act(() => chip.click());
    expect(client.connectMachine).toHaveBeenCalledWith("mini");
  });

  it("offers no Check again for this computer, which reconnects on its own", () => {
    const client = {
      machines: (): Machine[] => [{ id: "local", label: "This Mac", host: null, state: "error", error: "everyport-sidecar exited", snapshot: null }],
      subscribe: () => () => {},
      call: vi.fn(async () => {}),
      openUrl: vi.fn(async () => {}),
      connectMachine: vi.fn(async () => {}),
    } satisfies EveryportClient;
    const host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root!.render(<Popover client={client} />));
    expect(host.textContent).toContain("everyport-sidecar exited");
    expect([...host.querySelectorAll("button")].some((b) => b.textContent === "Check again")).toBe(false);
  });

  it("offers nothing to click while installing", () => {
    const { host, button } = render({ id: "devbox", label: "devbox", host: null, state: "installing", snapshot: null });
    expect(host.textContent).toContain("Installing Everyport on devbox…");
    expect(button("Install Everyport")).toBeUndefined();
  });
});
