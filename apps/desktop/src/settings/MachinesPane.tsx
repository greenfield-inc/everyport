import type { CheckStep, Machine } from "@everyport/protocol";
import { Checklist } from "@everyport/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { Row, Section } from "./controls";
import { useOnFocus } from "./useSettings";

/** `settings::MachineSettings` in the app's Rust side. */
type MachineSettings = {
  saved: { name: string; target: string }[];
  found: { name: string; source: string; command: string }[];
};

const STATUS: Record<Machine["state"], string> = {
  available: "Not connected",
  connecting: "Connecting…",
  install: "Everyport isn't installed",
  installing: "Installing Everyport…",
  connected: "Connected",
  error: "Can't connect",
};

/** Every machine the app watches, with its live status from the `machines` event. */
function useMachineStatus() {
  const [machines, setMachines] = useState<Machine[]>([]);
  useEffect(() => {
    const off = listen<Machine[]>("machines", ({ payload }) => setMachines(payload));
    void invoke<Machine[]>("machines_list").then(setMachines);
    return () => void off.then((f) => f());
  }, []);
  return (id: string) => machines.find((m) => m.id === id);
}

export function MachinesPane() {
  const status = useMachineStatus();
  const [list, setList] = useState<MachineSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(
    () => void invoke<MachineSettings>("settings_machines").then(setList, (e: unknown) => setError(String(e))),
    [],
  );
  useOnFocus(load);

  /** Runs a change and shows why it failed. Resolves to whether it worked. */
  const run = (command: string, args: Record<string, string>) =>
    invoke<void>(command, args).then(
      () => {
        setError(null);
        load();
        return true;
      },
      (e: unknown) => {
        setError(String(e));
        return false;
      },
    );

  return (
    <>
      {error && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      <Section title="How machines connect">
        <ol className="settings-steps">
          <li>Turn on SSH on the machine. On a Mac, that's Remote Login in Sharing settings.</li>
          <li>
            Let it accept your SSH key: run <code>ssh-copy-id</code> with the machine's name.
          </li>
          <li>Pick it here or in the menu, and Everyport installs itself there.</li>
        </ol>
      </Section>
      <Section>
        <MachineRow name="This computer" detail={null} machine={status("local")} />
        {list?.saved.map(({ name, target }) => (
          <MachineRow key={name} name={name} detail={target} machine={status(name)}>
            <button type="button" className="settings-button" onClick={() => void run("machine_remove", { name })}>
              Remove
            </button>
          </MachineRow>
        ))}
      </Section>
      <Section title="Add a machine">
        <AddMachine onAdd={(name, target) => run("machine_add", { name, target })} />
      </Section>
      {list && list.found.length > 0 && (
        <Section title="Found on this computer">
          {list.found.map(({ name, source, command }) => (
            <Row key={name} label={name} caption={`${source} · ${command}`}>
              <button type="button" className="settings-button" onClick={() => void run("machine_add", { name, target: command })}>
                Add
              </button>
            </Row>
          ))}
        </Section>
      )}
    </>
  );
}

function MachineRow({ name, detail, machine, children }: { name: string; detail: string | null; machine?: Machine; children?: React.ReactNode }) {
  const [confirming, setConfirming] = useState(false);
  const [checked, setChecked] = useState<CheckStep[] | string | null>(null);
  const [checking, setChecking] = useState(false);
  const check = () => {
    setChecking(true);
    invoke<CheckStep[]>("check_machine", { machineId: name })
      .then(setChecked, (e: unknown) => setChecked(String(e)))
      .finally(() => setChecking(false));
  };
  const state = machine?.state;
  const status = state ? (state === "error" && machine.error ? machine.error : STATUS[state]) : null;
  const steps = typeof checked === "object" && checked ? checked : state === "error" ? machine?.check : undefined;
  const caption = [detail, status].filter(Boolean).join(" · ");
  const offer = state === "install" ? machine?.install : undefined;
  return (
    <>
      <Row label={name} caption={caption || undefined}>
        {offer && !confirming && (
          <button type="button" className="settings-button" onClick={() => setConfirming(true)}>
            Install Everyport…
          </button>
        )}
        {name !== "This computer" && machine && (
          <button type="button" className="settings-button" disabled={checking} onClick={check}>
            {checking ? "Checking…" : "Check"}
          </button>
        )}
        {children}
      </Row>
      {steps && (
        <div className="settings-check">
          <Checklist steps={steps} />
        </div>
      )}
      {typeof checked === "string" && <p className="settings-error-text settings-check">{checked}</p>}
      {offer && confirming && (
        <div className="settings-confirm" role="alertdialog" aria-label={`Install Everyport on ${name}`}>
          <span>
            Install Everyport {offer.version} at {offer.path} on {name}? The app copies it over the same connection and checks
            its checksum.
          </span>
          <button type="button" className="settings-button" onClick={() => setConfirming(false)}>
            Cancel
          </button>
          <button
            type="button"
            className="settings-button settings-primary"
            onClick={() => {
              setConfirming(false);
              void invoke("install_everyport", { machineId: machine!.id });
            }}
          >
            Install
          </button>
        </div>
      )}
    </>
  );
}

/** A name, and a command prefix or an `everyport://` code from `everyport serve`. */
function AddMachine({ onAdd }: { onAdd: (name: string, target: string) => Promise<boolean> }) {
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void onAdd(name, target).then((added) => {
      if (!added) return;
      setName("");
      setTarget("");
    });
  };
  return (
    <form className="settings-add" onSubmit={submit}>
      <input aria-label="Name" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
      <input
        aria-label="Command or code"
        placeholder="ssh devbox, or an everyport:// code"
        value={target}
        onChange={(e) => setTarget(e.target.value)}
      />
      <button type="submit" className="settings-button settings-primary" disabled={!name.trim() || !target.trim()}>
        Add
      </button>
    </form>
  );
}
