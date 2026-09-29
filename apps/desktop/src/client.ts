import type { AgentSession, Call, Machine, EveryportClient } from "@everyport/protocol";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type Base, MachineUpdates, type Update } from "./updates";

/**
 * EveryportClient over the app's Rust side, which runs `everyport stdio` through the
 * sidecar and sends every machine's state as `machines` updates.
 */
export class TauriEveryportClient implements EveryportClient {
  #listeners = new Set<(machines: Machine[]) => void>();
  #updates = new MachineUpdates(
    () => invoke<Base>("machines_sync"),
    (machines) => {
      for (const listener of this.#listeners) listener(machines);
    },
  );

  constructor() {
    // Listening first, so no update falls between the base and the first event.
    void listen<Update>("machines", (event) => this.#updates.receive(event.payload)).then(() => this.#updates.resync());
  }

  machines() {
    return this.#updates.machines;
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

  installEveryport(machineId: string) {
    return invoke<void>("install_everyport", { machineId });
  }

  openSettings() {
    void invoke("open_settings");
  }
}
