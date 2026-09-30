import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

/** `updater::Status` in the app's Rust side. */
export type UpdaterStatus = {
  current: string;
  /** The newest release, once a check has found it. */
  latest: string | null;
  /** `latest` when it's newer than this app. */
  available: string | null;
  /** `available` unless the user skipped it. */
  offer: string | null;
  checking: boolean;
  /** Why the last check failed. */
  error: string | null;
  /** What the user has to do when the update can't run on its own. */
  manual: { kind: "paste"; command: string } | { kind: "package"; url: string } | null;
};

/** The updater's status, kept live by the `updater` event. Null until loaded. */
export function useUpdater(): UpdaterStatus | null {
  const [status, setStatus] = useState<UpdaterStatus | null>(null);
  useEffect(() => {
    let live = false;
    const off = listen<UpdaterStatus>("updater", ({ payload }) => {
      live = true;
      setStatus(payload);
    });
    void invoke<UpdaterStatus>("updater_status").then((current) => {
      if (!live) setStatus(current);
    });
    return () => void off.then((f) => f());
  }, []);
  return status;
}

export const installUpdate = () => void invoke("updater_install");
