# Intent brief: Everyport v1

## Goal

Ship the product the [README](../README.md) describes. It's a cross-platform, open-source port of [WhatThePort](../reference/what-the-port) that:

- feels native on macOS, Windows and Linux
- watches remote machines through any command or URL
- can later drop into Pane

The README is the spec for what users see. This brief covers why, the constraints, and how to make it feel native.

## Who it's for

Developers who run several dev servers at once, often started by coding agents in worktrees, and forget them. Their laptop slows down, and they don't know which `node` process is which. They also run agents on remote VMs and boxes, and want one view of all of it.

## Decisions

| Decision | Choice |
|---|---|
| Stack | Tauri 2, React 19, Tailwind 4. Rust for everything that touches the OS. |
| Process model | The desktop app never scans in-process. It runs `everyport stdio` as a sidecar locally, and the same binary through a connection command remotely. One code path for every machine. |
| Protocol | JSON over stdio or HTTP+SSE. Defined once in `crates/everyport/src/protocol.rs`, exported to TypeScript. |
| UI package | `packages/ui` in this repo. React only, no Tauri imports, so Pane can embed it later. Extract to a shared Dcouple package only when a second product needs it. |
| Look | Layout, spacing, type sizes and radii match the Paper design pixel for pixel (`docs/design/`). Colors and the body font come from the active Doozy theme. Numbers stay monospace. Animations are ours. See [design-spec.md](design-spec.md). |
| Remote | Any command prefix (`ssh`, `docker exec -i`, `kubectl exec -i --`, `wsl --`, custom) or an `everyport serve` URL. Hosts come from `~/.ssh/config`, Pane remote hosts, and manual entry. Remote OSes: Linux, macOS, Windows. |
| Windows + WSL | First-class. On Windows the app finds every installed WSL distro and adds each one as a machine over `wsl.exe -d <distro> --exec`, with `everyport` installed inside the distro. Ports that Windows sees as `wslrelay.exe` belong to the distro's server, not a separate Windows row. Path, quoting and launch rules follow Pane's (`main/src/utils/wslUtils.ts` in [greenfield-inc/Pane](https://github.com/greenfield-inc/Pane)). |
| Remote install | The app offers to install `everyport` on first connect. It pipes the binary through the connection, checks SHA-256, and puts it in `~/.local/bin`. |
| v1 scope | Core monitor, agent and Vercel links, terminal CLI and TUI, remote machines. Localization and auto-update come after v1. |
| Privacy | No telemetry. The app is online only for Vercel lookups through `gh`, opt-in. |
| Names | Product "Everyport", binary `everyport`, repo `greenfield-inc/everyport`, packages `everyport` on npm, crates.io and PyPI. |

## Budgets

Measure these on an Apple Silicon Mac with 5 servers running, and put the numbers in the PR that meets them.

| Metric | Budget |
|---|---|
| Popover open, warm (click to first painted frame) | under 50 ms |
| Cold start to tray icon visible | under 400 ms |
| Idle memory, app + webview processes + sidecar | under 60 MB |
| Idle CPU, popover hidden | under 0.3% of one core |
| Scan cost at the 2 s interval | under 1% of one core in `everyport` |
| Snapshot to UI update | under 16 ms of main-thread work |

## Native feel

These are what separate "a web page in a window" from an app that belongs in the menu bar. Each one is required unless it says otherwise.

### All platforms

1. **Pre-create the popover window at launch, hidden, and never destroy it.** Toggle it with show and hide. This is the biggest factor in making it open instantly.
2. **Show only a finished frame.** At launch, keep the window hidden until the web side sends `ready` after its first render with data. Every later open shows the last rendered state at once, and new data swaps in without a flash.
3. **Hide on blur.** Clicking anywhere else, pressing Escape, or clicking the tray icon again closes it. Reopening restores the same view and scroll position for 60 s, then resets to the list.
4. **Keep scanning out of the UI process.** `everyport` runs as a sidecar. The UI gets whole snapshots and diffs them with React keys by port. No polling from JS.
5. **Pause when hidden.** When the popover is hidden, the web side stops animations and chart work, and the sidecar keeps its normal interval but the app skips forwarding snapshots except the tray count and alerts. Resume on show with the latest snapshot.
6. **Transparent, frameless, fixed-size window** with no taskbar entry: `decorations: false`, `transparent: true`, `resizable: false`, `skipTaskbar: true`, `alwaysOnTop: true`, `visible: false`. The web page draws the panel, its radius and its hairline border. On macOS and Windows the OS draws the shadow.
7. **App chrome behavior in CSS.** Use `user-select: none` everywhere except copyable values, `cursor: default` (pointer only on links), `overscroll-behavior: none`, and no default context menu (right-click opens our own menu, or nothing). Show focus rings only on `:focus-visible`. Use tabular numbers (`font-variant-numeric: tabular-nums`) and `-webkit-font-smoothing: antialiased`.
8. **Full keyboard use.** Up and Down move the selection, Enter opens detail, Left or Escape goes back, ⌘/Ctrl+O opens the URL, ⌘/Ctrl+Backspace stops, ⌘/Ctrl+R restarts. The global shortcut ⌥⌘P or Ctrl+Alt+P toggles the popover (`tauri-plugin-global-shortcut`).
9. **One instance** (`tauri-plugin-single-instance`). Launching again opens the popover.
10. **Launch at login** is off by default and offered in onboarding (`tauri-plugin-autostart`).
11. **Tray right-click opens a native menu** (Tauri `Menu`): Open, Settings, Launch at login, Quit.
12. **Settings is a normal, decorated window**, created on first open and then kept like the popover.
13. **Motion is ours, short and interruptible.** Open: 140 ms opacity 0→1 and scale 0.98→1 from the tray edge. View changes: 180 ms slide with a spring. Rows: height and opacity on add and remove. Honor `prefers-reduced-motion`. Motion must never delay input.
14. **Theme follows the OS** (`prefers-color-scheme`, plus Tauri's theme-changed event) unless the user picks light or dark.
15. **Accessibility.** Rows are a listbox with options, and buttons have labels. VoiceOver, Narrator and Orca read the port, project and memory.

### macOS

16. **No Dock icon or app switcher entry:** `ActivationPolicy::Accessory`.
17. **Make the popover an `NSPanel`** with [`tauri-nspanel`](https://github.com/ahkohd/tauri-nspanel). Use a non-activating panel style, so it takes key focus without activating the app or stealing focus from the frontmost app. Set its level above the menu bar and its collection behavior to `canJoinAllSpaces | fullScreenAuxiliary | stationary`. It then opens over full-screen apps on the current Space. Close it from the panel delegate's `windowDidResignKey`.
18. **Native blur:** [`window-vibrancy`](https://github.com/tauri-apps/window-vibrancy) `apply_vibrancy(NSVisualEffectMaterial::Popover, Some(NSVisualEffectState::Active), Some(16.0))`. Set `macOSPrivateApi: true` in `tauri.conf.json`, which transparent windows need. The page background stays transparent, and the theme's `--popover` color sits on top as a translucent tint (about 72% opacity in dark mode, 80% in light).
19. **Position under the tray icon.** Use the tray click event's icon rect, centered and clamped to the visible frame of the screen that holds the icon, 6 pt below the menu bar. Recompute on every show, because the menu bar and display can change.
20. **Tray icon:** the socket mark ([brand/](../brand/README.md)) as an 18 pt template image so macOS tints it, with the right slot faint when idle. The server count is the tray title (`set_title`). Attention state uses a non-template amber image. Windows and Linux get a color icon for a light or dark panel, with the right slot green while servers run.
21. **Notifications** use the system notification center with actions Details, Stop and Snooze 1h. If the Tauri notification plugin can't deliver action buttons on desktop, show a borderless, non-activating notification panel with the same look, top right, for 8 s.

### Windows

22. **Flyout above the tray icon.** Position from the tray rect and the monitor work area, so it handles a taskbar on any edge. Hide on deactivate.
23. **Mica on Windows 11, Acrylic on 10:** `window-vibrancy` `apply_mica` or `apply_acrylic`. Request rounded corners with `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`.
24. **WebView2:** ship the Evergreen bootstrapper. Turn off browser accelerator keys and the default context menu with the WebView2 settings Tauri exposes.
25. **Toasts** go through the notification plugin, with the app's AppUserModelID set by the installer.

### Windows with WSL

26. **Each distro is a machine.** List distros with `wsl.exe -l -q` (the output is UTF-16LE), and connect with `wsl.exe -d <distro> --exec <path-to-everyport> stdio`. Call `wsl.exe` directly with an args array, never through `cmd.exe` or PowerShell.
27. **No duplicate rows.** A port whose Windows owner is `wslrelay.exe` or the WSL VM shows once, under its distro, with the Linux process tree.
28. **Paths cross the boundary correctly.** Linux paths show as Linux paths. Opening a folder uses `\\wsl.localhost\<distro>\...`, the editor uses `code --remote wsl+<distro> <linux path>`, and "resume in terminal" uses `wt.exe -p <distro>` or `wsl.exe -d <distro> --cd <dir>`. Quote for bash inside WSL, as Pane's `escapeForBash` does.
29. **Open URL works unchanged.** WSL2 forwards `localhost`, so `http://localhost:<port>` opens from Windows. Mirrored-networking mode works the same way.

### Linux

30. **Tray** is a StatusNotifierItem (libayatana-appindicator). Many desktops send left-click to the menu instead of the app, so the menu's first item is "Open Everyport". Where clicks do arrive (KDE, XFCE), toggle the popover directly.
31. **Positioning:** on X11, place it near the tray like on Windows. Wayland doesn't allow absolute positioning, so show it as a small undecorated window, centered at the top of the active output.
32. **No blur.** Use a solid `--popover` background. Avoid `backdrop-filter`, which is slow in WebKitGTK.
33. **WebKitGTK quirks:** if the window renders blank on NVIDIA, relaunch with `WEBKIT_DISABLE_DMABUF_RENDERER=1`, and document the fallback in `everyport doctor`.

## Disk work

Steady-state scans read no files. Process facts come from the kernel (libproc, `/proc`, Win32), which is not disk I/O. Everything read from disk is cached and invalidated by file events, following Pane's watcher (`main/src/services/gitFileWatcher.ts` in [greenfield-inc/Pane](https://github.com/greenfield-inc/Pane)):

- Watch only the files a result came from, not project trees: `.git/HEAD` (and a worktree's `gitdir` target) for the branch, the manifest (`package.json`, `Cargo.toml`, `pyproject.toml`) and framework config for name and framework, `.vercel/project.json`, and the one transcript file for an agent title. Use the `notify` crate (FSEvents, inotify, ReadDirectoryChangesW), non-recursive, on the containing folder.
- Batch events for about 1.5 s before re-reading.
- Run a 60 s self-heal tick that only calls `stat` (mtime and size), for events the OS coalesced or dropped.
- If watching fails (handle limits, WSL `/mnt` paths), fall back to that stat tick, never to re-reading files.
- Drop a watch and its cache entry when no server uses that path anymore.
- Discovery work (finding a transcript, listing session folders) runs once per new server, off the scan thread, never on a timer.

Budget: after startup, a scan with no file changes does zero file reads. Measure it with `fs_usage -f filesys` on macOS or `strace -e trace=file` on Linux, and paste the result in the PR.

## Engineering rules

- DRY and YAGNI. One code path for local and remote. No abstraction without two real users.
- The protocol is the contract. Change it only in `protocol.rs`, regenerate the types, and update `docs/protocol.md` and the fixture in the same PR.
- Tests check behavior through public interfaces, with expected values from the spec, the fixture or a worked example. No change-detector tests.
- Verify on this Mac when you can: run the app and screenshot every changed view next to its Paper frame. Windows and Linux are verified in CI, plus a VM if one is available.

## Done for v1

- Every README claim works as written, on all three OSes, including the install commands.
- The budgets above are met and measured.
- Screenshots of every view in light and dark mode sit next to their Paper frames.
- `everyport` watches a Linux VM over SSH, a Docker container, a Windows machine over SSH, and WSL distros from the Windows app.
