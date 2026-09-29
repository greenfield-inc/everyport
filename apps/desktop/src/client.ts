import type { AgentSession, Call, Machine, PpmClient } from "@ppm/protocol";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { merge } from "./updates";

/**
 * PpmClient over the app's Rust side, which runs `ppm stdio` through the
 * sidecar and sends every machine's state as `machines` updates.
 */
export class TauriPpmClient implements PpmClient {
  #machines: Machine[] = [];
  #listeners = new Set<(machines: Machine[]) => void>();
  #resyncing = false;

  constructor() {
    void listen<Machine[]>("machines", (event) => this.#apply(event.payload));
    this.#resync();
  }

  /** Gets every machine in full, after which updates.rs sends every server in full once. */
  #resync() {
    if (this.#resyncing) return;
    this.#resyncing = true;
    void invoke<Machine[]>("machines_list")
      .then((machines) => this.#apply(machines))
      .finally(() => (this.#resyncing = false));
  }

  #apply(update: Machine[]) {
    const machines = merge(this.#machines, update, () => this.#resync());
    this.#machines = machines;
    for (const listener of this.#listeners) listener(machines);
  }

  machines() {
    return this.#machines;
  }

  subscribe(listener: (machines: Machine[]) => void) {
    this.#listeners.add(listener);
    return () => void this.#listeners.delete(listener);
  }

  call(machineId: string, call: Call) {
    return invoke<void>("call_machine", { machineId, call });
  }

  openUrl(machineId: string, port: number) {
    return invoke<void>("open_server_url", { machineId, port });
  }

  openWorkspace(machineId: string, port: number) {
    return invoke<void>("open_workspace", { machineId, port });
  }

  openExternal(url: string) {
    return invoke<void>("open_external", { url });
  }

  revealFolder(machineId: string, path: string) {
    return invoke<void>("reveal_folder", { machineId, path });
  }

  openInEditor(machineId: string, path: string) {
    return invoke<void>("open_in_editor", { machineId, path });
  }

  resumeSession(machineId: string, session: AgentSession) {
    return invoke<void>("resume_session", { machineId, session });
  }

  connectMachine(machineId: string) {
    return invoke<void>("connect_machine", { machineId });
  }

  installPpm(machineId: string) {
    return invoke<void>("install_ppm", { machineId });
  }

  openSettings() {
    void invoke("open_settings");
  }
}
