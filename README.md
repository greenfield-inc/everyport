# Port Process Manager

See every dev server running on your machine, or on any box you can reach, in your menu bar.

Port Process Manager (`ppm`) finds every local server listening on a port and shows what it is, which branch it's on, which agent started it, and what it's costing you. Stop the ones you forgot about in one click. It runs on macOS, Windows and Linux, and it watches remote machines over SSH, Docker, Kubernetes, WSL or anything else that can run a command.

Free and open source. No account. No telemetry.

<!-- Screenshot: docs/assets/popover.png, captured from the running app -->

## Features

**See what's running**
- Every listening dev server with its port, project, git branch or worktree, framework and uptime
- Memory and CPU for the whole process tree behind each port, with a 10-minute history chart
- A total at the top: how much memory your servers use next to other apps and free memory

**Keep your machine fast**
- Spike alerts when a server's memory crosses your threshold or keeps climbing
- Clean up: one list of servers whose worktree is gone, that sat idle for hours, that ran for days, or that are leaking memory
- Auto-kill for those servers, set to ask first or act on its own
- Stop or restart any server. Protected processes such as databases are never touched.

**Jump to the work**
- Open `localhost:<port>` in your browser, or its Vercel preview for the same commit
- See the Claude Code or Codex session that started a server, and resume it in your terminal of choice
- Recognizes Conductor workspaces, Pane worktrees and plain git worktrees

**Watch any machine**
- Add remote machines from `~/.ssh/config`, your Pane remote profiles, or by hand
- Works over any command that runs a program: `ssh`, `docker exec`, `kubectl exec`, `wsl`, or your own
- Installs itself on the remote the first time you connect. Nothing to set up on the box.
- Opening a remote server's URL forwards the port for you

**Feels native**
- Opens instantly from the menu bar or tray, and from a global hotkey
- Floats over full-screen apps without stealing focus, with the system's own blur behind it
- Follows light and dark mode, with 40 color themes
- Uses about as much memory as a small native app

**Works in your terminal too**
- `ppm` opens the same server list as a terminal UI
- `ppm list --json` and `ppm watch --jsonl` for scripts and other tools

## Install

### Desktop app

| Platform | Install |
|---|---|
| macOS 13+ | `brew install --cask greenfield-inc/tap/port-process-manager` or download the `.dmg` from [Releases](https://github.com/greenfield-inc/port-process-manager/releases/latest) |
| Windows 10/11 | `winget install Greenfield.PortProcessManager` or download the `.msi` from [Releases](https://github.com/greenfield-inc/port-process-manager/releases/latest) |
| Linux | `.deb`, `.rpm` or `.AppImage` from [Releases](https://github.com/greenfield-inc/port-process-manager/releases/latest) |

On GNOME, the tray icon needs the [AppIndicator extension](https://extensions.gnome.org/extension/615/appindicator-support/).

### CLI only

Use the CLI on servers, VMs and containers, or if you prefer the terminal.

```bash
curl -fsSL https://github.com/greenfield-inc/port-process-manager/releases/latest/download/install.sh | sh
brew install greenfield-inc/tap/ppm
npx port-process-manager          # or: npm i -g port-process-manager
uvx port-process-manager          # or: pipx install port-process-manager
cargo install port-process-manager
```

Every method installs the same single binary, `ppm`, with no runtime dependencies.

## Quick start

1. Open Port Process Manager. Its icon shows how many servers are running.
2. Click the icon to see your servers. Click a server for details, charts and its process tree.
3. Click **Clean up** to stop the servers you no longer need.

Press <kbd>⌥</kbd> <kbd>⌘</kbd> <kbd>P</kbd> (<kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd> on Windows and Linux) to open it from anywhere. Change the shortcut in **Settings → General**.

## Remote machines

Open **Settings → Machines** and add a machine. Hosts from `~/.ssh/config` and your Pane remote profiles are listed for you. Pick one and the menu shows its servers next to your local ones.

A machine is any command that runs a program on it. These all work:

| Connection | Command |
|---|---|
| SSH | `ssh devbox` |
| Docker | `docker exec -i my-container` |
| Kubernetes | `kubectl exec -i my-pod --` |
| WSL | `wsl -d Ubuntu --` |
| Anything else | your own command prefix |

On first connect, the app checks the remote OS and CPU, copies the matching `ppm` binary to `~/.local/bin`, and keeps it up to date after that. If you'd rather install it yourself, use any command from [CLI only](#cli-only).

From the terminal:

```bash
ppm remote add devbox -- ssh devbox
ppm --on devbox                   # terminal UI for devbox
ppm --on devbox list --json
```

### Serve over the network

For browsers, Tailscale Serve or a reverse proxy, run `ppm` as a small HTTP server:

```bash
ppm serve                         # listens on 127.0.0.1:7767, prints a connection code
tailscale serve --bg http://127.0.0.1:7767
```

`ppm serve` only listens on loopback and requires the bearer token in the connection code. Paste the code into **Settings → Machines → Add by code**.

## CLI

```text
ppm                        Terminal UI with your servers
ppm list [--json]          List servers once
ppm watch --jsonl          Stream a snapshot on every change
ppm stop <port>            Stop the server on a port (--force to kill)
ppm restart <port>         Stop it, then run its command again in the same folder
ppm open <port>            Open http://localhost:<port>
ppm clean [--yes]          Stop the servers Clean up suggests
ppm stdio                  Speak the ppm protocol on stdin/stdout
ppm serve                  Speak the ppm protocol over HTTP on loopback
ppm remote add|list|rm     Manage remote machines
ppm --on <machine> ...     Run any command against a remote machine
ppm doctor                 Check permissions and platform support
```

## Build on ppm

`ppm` speaks one JSON protocol, over stdin/stdout (`ppm stdio`) or HTTP with server-sent events (`ppm serve`). Anything that can run a command or open a URL can show your servers: an editor extension, a status bar, a dashboard, or a workspace app like [Pane](https://runpane.com). See [docs/protocol.md](docs/protocol.md).

```bash
ppm watch --jsonl | jq '.servers[] | {port, project: .project.name, memory}'
```

## Build from source

You need Rust (stable), Node.js 20+ and pnpm.

```bash
git clone https://github.com/greenfield-inc/port-process-manager
cd port-process-manager
pnpm install
pnpm dev            # run the desktop app
cargo run -p port-process-manager -- list   # run the CLI
```

See [AGENTS.md](AGENTS.md) for the repo layout and checks.

## Credits

Port Process Manager started as a port of [WhatThePort](https://github.com/tomjohndesign/what-the-port) by Tomjohn Design, a macOS app released under the MIT license. It is part of the [Greenfield](https://greenfield.to) family with Pane, Grain, Doozy and Agent Farm.

## License

[MIT](LICENSE)
