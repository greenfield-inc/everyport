import type { AgentSession, Call, Event, HostInfo, Snapshot } from "./generated/index.ts";

/** The id of this computer, which the host app runs itself and reconnects on its own. */
export const LOCAL_MACHINE = "local";

/** One machine the UI shows: this computer, or a remote connection. */
export interface Machine {
  id: string;
  label: string;
  host: HostInfo | null;
  /**
   * `available`: discovered, and connects when picked (`connectMachine`).
   * `install`: everyport isn't on the machine; `install` says what installing does,
   * and `installEveryport` does it once the user says yes.
   */
  state: "available" | "connecting" | "install" | "installing" | "connected" | "error";
  error?: string;
  /** The steps to reach the machine, up to the one that failed, while it can't connect. */
  check?: CheckStep[];
  install?: { version: string; path: string };
  snapshot: Snapshot | null;
}

/** One step of reaching a machine, from `everyport::client::check`. */
export interface CheckStep {
  label: string;
  ok: boolean;
  /** One sentence with the fix, for the failed step. Commands are in `backticks`. */
  fix?: string;
  /** What the failing tool printed. */
  detail?: string;
}

/**
 * What the UI needs from its host app. The Tauri app implements it with
 * `everyport::client`; Pane can implement it over its daemon; tests use the fixture.
 * @everyport/ui imports only this interface, never Tauri.
 */
export interface EveryportClient {
  machines(): Machine[];
  subscribe(listener: (machines: Machine[]) => void): () => void;
  call(machineId: string, call: Call): Promise<void>;
  openUrl(machineId: string, port: number): Promise<void>;
  onEvent?(listener: (machineId: string, event: Event) => void): () => void;
  /** Opens `workspace.open_url` (such as pane://) for the server on this port. */
  openWorkspace?(machineId: string, port: number): Promise<void>;
  /** Opens a web URL, such as a Vercel preview, in the default browser. */
  openExternal?(url: string): Promise<void>;
  revealFolder?(machineId: string, path: string): Promise<void>;
  openInEditor?(machineId: string, path: string): Promise<void>;
  resumeSession?(machineId: string, session: AgentSession): Promise<void>;
  openSettings?(): void;
  /** Connects a discovered machine the user picked, or tries a failed one again. */
  connectMachine?(machineId: string): Promise<void>;
  /** Installs everyport on a machine in the `install` state, then connects. */
  installEveryport?(machineId: string): Promise<void>;
}
