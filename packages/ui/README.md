# @ppm/ui

The Port Process Manager popover as React components. It imports only React and `@ppm/protocol`, so any host can embed it: the desktop app, or Pane later.

```tsx
import { Popover } from "@ppm/ui";
import "@ppm/ui/styles.css"; // the host runs Tailwind 4 (@tailwindcss/vite)

<Popover client={client} theme="pastel-dreams-green" appearance="system" onReady={show} />;
```

| Export | What it is |
|---|---|
| `Popover` | List, detail and Clean up for every machine `client` knows. Props: `client`, `theme`, `appearance`, `alertMemory`, `initialServer`, `onReady`, `autoFocus` (false on a page that embeds it) |
| `NotificationCard` | The memory alert with Details, Stop and Snooze 1h, for hosts that draw their own notification |
| `ServerList`, `ServerDetail`, `CleanUp`, `MachineSwitcher` | The views, each taking a `ViewContext` from `useViewContext(client, machine)` |
| `themeList`, `Themed` | The 40 Doozy themes, and the root that applies one |

Actions the client leaves out (`openWorkspace`, `openExternal`, `revealFolder`, `openInEditor`, `resumeSession`, `openSettings`) are hidden.

## Host hooks

- `html[data-vibrancy]`: native blur sits behind the page, so the panel tints `--popover` to 72% (dark) or 80% (light) instead of drawing it solid.
- `html[data-hidden]`: the popover is hidden, so animation pauses.
- `--ppm-shadow: none`: set this inside a native window, where the OS draws the shadow.
- Escape goes back from detail and Clean up and calls `preventDefault()`. On the list the host sees it, and can hide the popover.

## Develop

```bash
pnpm --filter @ppm/ui dev           # playground: every view from the fixture, with theme and mode pickers
pnpm --filter @ppm/ui test
pnpm --filter @ppm/ui screenshots   # 2x shots next to each Paper frame, and docs/assets/popover.png
pnpm --filter @ppm/ui themes        # after copying a new src/themes/registry.json from Doozy
```
