// Renders every view from the fixture. URL parameters pick the state, so the
// screenshot script can load each one directly:
//   scenario  paper | protected | machines | empty
//   view      popover | notification
//   port      open this server's detail
//   theme     a theme name;  mode  light | dark | system
//   live      advance snapshots every 2 s;  shot  hide the toolbar
import { fixtureSnapshot } from "@everyport/protocol";
import { StrictMode, useMemo } from "react";
import { createRoot } from "react-dom/client";
import { type Appearance, DEFAULT_THEME, NotificationCard, Popover, themeList } from "../src/index.ts";
import "../src/styles.css";
import { fixtureClient, scenarios } from "./scenarios.ts";

const params = new URLSearchParams(location.search);
const get = (key: string, fallback: string) => params.get(key) ?? fallback;

function navigate(key: string, value: string | null) {
  const next = new URLSearchParams(location.search);
  if (value === null) next.delete(key);
  else next.set(key, value);
  location.search = next.toString();
}

function Playground() {
  const scenario = get("scenario", "paper");
  const view = get("view", "popover");
  const theme = get("theme", DEFAULT_THEME);
  const mode = get("mode", "dark") as Appearance;
  const port = params.get("port");
  const shot = params.has("shot");
  const client = useMemo(() => fixtureClient((scenarios[scenario] ?? scenarios.paper)(), params.has("live") ? 2000 : null), [scenario]);
  (window as unknown as { everyport: typeof client }).everyport = client;

  const dark = mode === "dark" || (mode === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  const leaking = fixtureSnapshot.servers.find((server) => server.status === "attention")!;

  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-end",
        gap: 16,
        padding: shot ? 24 : "16px 16px 48px",
        font: "12px system-ui",
        color: dark ? "#ddd" : "#222",
        background: dark
          ? "radial-gradient(120% 80% at 10% 0%, #2a3242, transparent 60%), radial-gradient(90% 70% at 100% 100%, #3a3020, transparent 60%), #0b0d12"
          : "radial-gradient(120% 80% at 10% 0%, #dfe7f5, transparent 60%), radial-gradient(90% 70% at 100% 100%, #f3e6cf, transparent 60%), #eef0f4",
      }}
    >
      {!shot && (
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignSelf: "stretch", alignItems: "center" }}>
          <select value={scenario} onChange={(event) => navigate("scenario", event.target.value)}>
            {Object.keys(scenarios).map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
          <select value={view} onChange={(event) => navigate("view", event.target.value)}>
            <option>popover</option>
            <option>notification</option>
          </select>
          <select value={theme} onChange={(event) => navigate("theme", event.target.value)}>
            {themeList.map(({ name, title }) => (
              <option key={name} value={name}>
                {title} ({name})
              </option>
            ))}
          </select>
          <select value={mode} onChange={(event) => navigate("mode", event.target.value)}>
            <option>dark</option>
            <option>light</option>
            <option>system</option>
          </select>
          <label>
            <input type="checkbox" checked={params.has("live")} onChange={(event) => navigate("live", event.target.checked ? "1" : null)} /> live
          </label>
          <span style={{ opacity: 0.6 }}>Keys: ↑ ↓ Enter ← Esc, ⌘O ⌘⌫ ⌘R. Actions log to the console.</span>
        </div>
      )}
      {view === "notification" ? (
        <NotificationCard
          theme={theme}
          appearance={mode}
          server={leaking}
          alert={{ port: leaking.port, kind: "leaking", memory: leaking.memory }}
          onDetails={() => console.info("[everyport] details")}
          onStop={() => console.info("[everyport] stop")}
          onSnooze={() => console.info("[everyport] snooze")}
        />
      ) : (
        <Popover
          client={client}
          theme={theme}
          appearance={mode}
          initialServer={port ? { machineId: "local", port: Number(port) } : undefined}
          onReady={() => document.documentElement.setAttribute("data-ready", "")}
        />
      )}
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Playground />
  </StrictMode>,
);
