import type { Call, Event, HostInfo, Snapshot } from "./generated/index.ts";

/** One machine the UI shows: this computer, or a remote connection. */
export interface Machine {
  id: string;
  label: string;
  host: HostInfo | null;
  state: "connecting" | "connected" | "installing" | "error";
  error?: string;
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
}
