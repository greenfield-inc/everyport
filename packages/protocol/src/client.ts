import type { AgentSession, Call, Event, HostInfo, Snapshot } from "./generated/index.ts";

/** One machine the UI shows: this computer, or a remote connection. */
export interface Machine {
  id: string;
  label: string;
  host: HostInfo | null;
  /**
   * `available`: discovered, and connects when picked (`connectMachine`).
   * `install`: ppm isn't on the machine; `install` says what installing does,
   * and `installPpm` does it once the user says yes.
   */
  state: "available" | "connecting" | "install" | "installing" | "connected" | "error";
  error?: string;
  install?: { version: string; path: string };
  snapshot: Snapshot | null;
}

/**
 * What the UI needs from its host app. The Tauri app implements it with
 * ppm-client; Pane can implement it over its daemon; tests use the fixture.
 * @ppm/ui imports only this interface, never Tauri.
 */
export interface PpmClient {
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
  /** Connects a discovered machine the user picked. */
  connectMachine?(machineId: string): Promise<void>;
  /** Installs ppm on a machine in the `install` state, then connects. */
  installPpm?(machineId: string): Promise<void>;
}
