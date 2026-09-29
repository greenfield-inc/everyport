// @vitest-environment jsdom
// Other machines through a rendered <Popover>: the README's "The app offers
// to install ppm on a machine the first time you connect", and discovered
// hosts that connect once picked.
import { fixtureSnapshot, type Machine, type PpmClient } from "@ppm/protocol";
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
    installPpm: vi.fn(async () => {}),
  } satisfies PpmClient;
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root!.render(<Popover client={client} initialServer={{ machineId: remote.id, port: 0 }} />));
  const button = (label: string) => [...host.querySelectorAll("button")].find((b) => b.textContent === label);
  return { client, host, button };
}

describe("another machine", () => {
  it("asks before installing ppm, and installs on yes", () => {
    const { client, host, button } = render({
      id: "devbox",
      label: "devbox",
      host: null,
      state: "install",
      install: { version: "0.1.0", path: "/home/me/.local/bin/ppm" },
      snapshot: null,
    });
    expect(host.textContent).toContain("ppm isn't on devbox yet. Install ppm 0.1.0 to /home/me/.local/bin/ppm?");
    expect(client.installPpm).not.toHaveBeenCalled();
    act(() => button("Install ppm")!.click());
    expect(client.installPpm).toHaveBeenCalledWith("devbox");
  });

  it("shows why an install failed, next to the same question", () => {
    const { host, button } = render({
      id: "devbox",
      label: "devbox",
      host: null,
      state: "install",
      error: "Couldn't install ppm: no space left on device",
      install: { version: "0.1.0", path: "/home/me/.local/bin/ppm" },
      snapshot: null,
    });
    expect(host.textContent).toContain("Couldn't install ppm: no space left on device");
    expect(button("Install ppm")).toBeDefined();
  });

  it("connects a discovered host when picked", () => {
    const { client, host, button } = render({ id: "mini", label: "mini", host: null, state: "available", snapshot: null });
    expect(host.textContent).toContain("mini isn't connected.");
    act(() => button("Connect")!.click());
    expect(client.connectMachine).toHaveBeenCalledWith("mini");
  });

  it("offers nothing to click while installing", () => {
    const { host, button } = render({ id: "devbox", label: "devbox", host: null, state: "installing", snapshot: null });
    expect(host.textContent).toContain("Installing ppm on devbox…");
    expect(button("Install ppm")).toBeUndefined();
  });
});
