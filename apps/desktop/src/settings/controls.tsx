import { type ReactNode, useId } from "react";

/** A setting: its label and an optional caption on the left, the control on the right. */
export function Row({ label, caption, children }: { label: string; caption?: ReactNode; children: ReactNode }) {
  const id = useId();
  return (
    <div className="settings-row" role="group" aria-labelledby={id}>
      <div className="settings-label">
        <span id={id}>{label}</span>
        {caption && <small>{caption}</small>}
      </div>
      <div className="settings-control">{children}</div>
    </div>
  );
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      {title && <h2>{title}</h2>}
      <div className="settings-group">{children}</div>
    </section>
  );
}

export function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <input
      type="checkbox"
      role="switch"
      className="settings-switch"
      aria-label={label}
      checked={checked}
      onChange={(event) => onChange(event.target.checked)}
    />
  );
}

export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="settings-segmented" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Select({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: number;
  options: readonly { value: number; label: string }[];
  onChange: (value: number) => void;
}) {
  // A value set by hand in config.toml still shows.
  const all = options.some((o) => o.value === value) ? options : [...options, { value, label: String(value) }];
  return (
    <select aria-label={label} value={value} onChange={(event) => onChange(Number(event.target.value))}>
      {all.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

/** Seconds, hours or days as "5 seconds", "1 hour", "3 days". */
export const count = (n: number, unit: string) => `${n} ${unit}${n === 1 ? "" : "s"}`;
