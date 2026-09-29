import type { AutoKill, Config } from "@everyport/protocol";
import { useState } from "react";
import { Row, Section, Segmented, Select, count } from "./controls";
import { type Settings, saveConfig } from "./useSettings";

const MB = 1024 ** 2;
const GB = 1024 ** 3;
const HOUR = 3600;
const DAY = 24 * HOUR;

const THRESHOLDS = [0.5, 1, 2, 4, 8, 16].map((gb) => ({ value: gb * GB, label: gb < 1 ? `${gb * 1024} MB` : `${gb} GB` }));
const GROWTH = [250, 500, 1000, 2000].map((mb) => ({ value: mb * MB, label: mb < 1000 ? `${mb} MB` : `${mb / 1000} GB` }));
const IDLE = [1, 2, 4, 8, 24].map((h) => ({ value: h * HOUR, label: count(h, "hour") }));
const RUNNING = [1, 3, 7, 14].map((d) => ({ value: d * DAY, label: count(d, "day") }));

const AUTO_KILL = [
  { value: "off", label: "Off" },
  { value: "ask", label: "Ask" },
  { value: "act", label: "Stop them" },
] as const satisfies readonly { value: AutoKill; label: string }[];

const AUTO_KILL_CAPTION: Record<AutoKill, string> = {
  off: "They wait under Clean up.",
  ask: "When a server starts to qualify, a notification asks before stopping it.",
  act: "Servers that start to qualify on this computer are stopped for you. Leaking ones are only listed.",
};

export function CleanUpPane({ settings: { config } }: { settings: Settings }) {
  const set = <K extends keyof Config>(key: K) => (value: Config[K]) => void saveConfig({ [key]: value });
  return (
    <>
      <Section title="Alerts">
        <Row label="Alert when a server uses more than">
          <Select label="Memory alert" value={config.alert_memory} options={THRESHOLDS} onChange={set("alert_memory")} />
        </Row>
        <Row label="Call it a leak when it grows by" caption="Within 10 minutes">
          <Select label="Leak growth" value={config.leak_growth} options={GROWTH} onChange={set("leak_growth")} />
        </Row>
      </Section>
      <Section title="Suggest stopping servers that">
        <Row label="Have been idle for" caption="No CPU and no open connections">
          <Select label="Idle for" value={config.idle_after_secs} options={IDLE} onChange={set("idle_after_secs")} />
        </Row>
        <Row label="Have been running for">
          <Select label="Running for" value={config.long_running_after_secs} options={RUNNING} onChange={set("long_running_after_secs")} />
        </Row>
        <Row label="When servers qualify" caption={AUTO_KILL_CAPTION[config.auto_kill]}>
          <Segmented label="Auto-kill" value={config.auto_kill} options={AUTO_KILL} onChange={set("auto_kill")} />
        </Row>
      </Section>
      <Section title="Never stop">
        <Row label="Protected processes" caption="Clean up and auto-kill skip servers run by these, and Stop asks first">
          <ProtectedList
            names={config.protected}
            onChange={(update) => void saveConfig((current) => ({ protected: update(current.protected) }))}
          />
        </Row>
      </Section>
    </>
  );
}

/** `onChange` gets an update to apply to the newest list, so quick edits don't undo each other. */
function ProtectedList({ names, onChange }: { names: string[]; onChange: (update: (names: string[]) => string[]) => void }) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const name = draft.trim();
    if (name) onChange((current) => (current.includes(name) ? current : [...current, name]));
    setDraft("");
  };
  return (
    <div className="settings-chips">
      {names.map((name) => (
        <span key={name} className="settings-chip">
          {name}
          <button type="button" aria-label={`Remove ${name}`} onClick={() => onChange((current) => current.filter((n) => n !== name))}>
            ×
          </button>
        </span>
      ))}
      <input
        aria-label="Add a protected process"
        placeholder="Add…"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && add()}
        onBlur={add}
      />
    </div>
  );
}
