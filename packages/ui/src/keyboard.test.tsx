// @vitest-environment jsdom
// Keyboard use from the intent brief, item 8, through a rendered <Popover>,
// and the class prefix every view relies on. jsdom reports no Mac platform,
// so the command key is Ctrl here.
import { fixtureSnapshot, type Machine, type PpmClient } from "@ppm/protocol";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Popover } from "./index.ts";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
window.matchMedia = ((query: string) => ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} })) as never;
Element.prototype.scrollIntoView = () => {};

let host: HTMLDivElement;
let root: Root;
let client: PpmClient & { call: ReturnType<typeof vi.fn>; openUrl: ReturnType<typeof vi.fn> };

beforeEach(() => {
  const machines: Machine[] = [{ id: "local", label: "This Mac", host: null, state: "connected", snapshot: fixtureSnapshot }];
  client = {
    machines: () => machines,
    subscribe: () => () => {},
    call: vi.fn(async () => {}),
    openUrl: vi.fn(async () => {}),
  };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root.render(<Popover client={client} />));
});

afterEach(() => {
  act(() => root.unmount());
  document.body.replaceChildren();
});

/** Sends a keydown to the focused element, as a user's key press would, and says whether it was taken. */
function press(key: string, options: KeyboardEventInit = {}, target: Element = document.activeElement ?? document.body) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  act(() => {
    target.dispatchEvent(event);
  });
  return event.defaultPrevented;
}

const title = () => document.querySelector("h1")?.textContent;
const selected = () => document.querySelector("[role=option][aria-selected=true]")?.getAttribute("data-port");

describe("keyboard", () => {
  it("moves the selection with the arrows, on the focused listbox a screen reader follows", () => {
    press("ArrowDown");
    press("ArrowDown");
    const listbox = document.querySelector("[role=listbox]");
    expect(document.activeElement).toBe(listbox);
    expect(selected()).toBe("3001");
    expect(listbox?.getAttribute("aria-activedescendant")).toBe("ppm-server-3001");
    expect(document.getElementById("ppm-server-3001")?.getAttribute("aria-label")).toBe("Port 3001, greenfield.to, 612 MB");
  });

  it("opens the detail with Enter and goes back with Escape, which the host doesn't see", () => {
    press("ArrowDown");
    press("Enter");
    expect(title()).toBe("port-process-manager");
    expect(press("Escape")).toBe(true);
    expect(title()).toBe("Servers");
    expect(selected()).toBe("3000");
  });

  it("leaves Escape on the list to the host, which hides the popover", () => {
    expect(press("Escape")).toBe(false);
  });

  it("opens, stops and restarts with the command key", () => {
    press("ArrowDown");
    press("o", { ctrlKey: true });
    expect(client.openUrl).toHaveBeenCalledWith("local", 3000);
    press("Backspace", { ctrlKey: true });
    expect(client.call).toHaveBeenCalledWith("local", expect.objectContaining({ method: "stop", params: expect.objectContaining({ port: 3000, confirm_protected: false }) }));
    press("Enter");
    press("r", { ctrlKey: true });
    expect(client.call).toHaveBeenCalledWith("local", expect.objectContaining({ method: "restart" }));
  });

  it("does nothing for keys typed into a host field", () => {
    press("ArrowDown");
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    expect(press("Backspace", { ctrlKey: true })).toBe(false);
    expect(press("ArrowDown")).toBe(false);
    expect(press("r", { ctrlKey: true })).toBe(false);
    expect(client.call).not.toHaveBeenCalled();
    expect(selected()).toBe("3000");
  });

  it("checks and unchecks Clean up rows with Space", () => {
    const cleanUp = [...document.querySelectorAll("button")].find((button) => button.textContent?.startsWith("Clean up"));
    act(() => cleanUp?.click());
    expect(title()).toBe("Clean up");
    const checked = () => [...document.querySelectorAll("[role=option][aria-selected=true]")].map((row) => row.getAttribute("data-port"));
    expect(checked()).toEqual(["8000", "5173"]);
    press("ArrowDown");
    press(" ");
    expect(checked()).toEqual(["5173"]);
  });
});

describe("protected servers", () => {
  // :3000 as a protected postgres, to stop or restart only after a confirm.
  beforeEach(() => {
    const servers = fixtureSnapshot.servers.map((server) => (server.port === 3000 ? { ...server, process_name: "postgres", protected: true } : server));
    const machines: Machine[] = [{ id: "local", label: "This Mac", host: null, state: "connected", snapshot: { ...fixtureSnapshot, servers } }];
    client = { ...client, machines: () => machines };
    act(() => root.unmount());
    root = createRoot(host);
    act(() => root.render(<Popover client={client} />));
    press("ArrowDown");
  });

  const alert = () => document.querySelector("[role=alert]")?.getAttribute("aria-label");
  const button = (label: string) => [...document.querySelectorAll("button")].find((b) => b.textContent === label || b.getAttribute("aria-label") === label);
  const params = (method: string) => client.call.mock.calls.find(([, call]) => call.method === method)?.[1].params;

  it("asks before the row's Stop, and stops on confirm", () => {
    act(() => button("Stop, protected")?.click());
    expect(alert()).toBe("postgres :3000 is protected. Stop it anyway?");
    expect(client.call).not.toHaveBeenCalled();
    act(() => button("Stop")?.click());
    expect(params("stop")).toMatchObject({ port: 3000, force: false, confirm_protected: true });
  });

  it("cancels the confirm with Escape and stops on a second ⌘⌫", () => {
    press("Backspace", { ctrlKey: true });
    expect(alert()).toBe("postgres :3000 is protected. Stop it anyway?");
    expect(press("Escape")).toBe(true);
    expect(alert()).toBeUndefined();
    press("Backspace", { ctrlKey: true });
    press("Backspace", { ctrlKey: true });
    expect(params("stop")).toMatchObject({ port: 3000, confirm_protected: true });
  });

  it("keeps the detail open while a restart waits for its confirm", () => {
    press("Enter");
    press("r", { ctrlKey: true });
    expect(alert()).toBe("postgres :3000 is protected. Restart it anyway?");
    expect(title()).toBe("port-process-manager");
    act(() => button("Cancel")?.click());
    expect(alert()).toBeUndefined();
    press("Backspace", { ctrlKey: true });
    expect(title()).toBe("port-process-manager");
    act(() => button("Stop")?.click());
    expect(params("stop")).toMatchObject({ confirm_protected: true });
    expect(client.call).toHaveBeenCalledTimes(1);
  });
});

describe("styles", () => {
  // Tailwind utilities only apply with the ppm: prefix. The rest are the
  // package's own classes from styles.css.
  const OWN = new Set(["ppm-root", "ppm-panel", "ppm-view", "ppm-grow", "ppm-scroll", "ppm-spin", "selectable"]);
  const unstyled = () =>
    [...document.querySelectorAll("[class]")].flatMap((element) =>
      [...element.classList].filter((name) => !name.startsWith("ppm:") && !OWN.has(name)),
    );
  const click = (text: string) => {
    const button = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent?.startsWith(text));
    act(() => button?.click());
  };

  it("prefixes every utility on the list, the expanded detail and Clean up", () => {
    press("ArrowDown");
    act(() => document.getElementById("ppm-server-3000")?.dispatchEvent(new PointerEvent("pointerover", { bubbles: true })));
    expect(unstyled()).toEqual([]);
    press("Enter");
    click("6 more");
    click("Processes");
    click("Dot-grid menu bar icon");
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    expect(unstyled()).toEqual([]);
    press("Escape");
    press("Escape");
    click("Clean up");
    expect(unstyled()).toEqual([]);
  });
});
