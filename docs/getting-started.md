# Getting started

## Install

Install the desktop app, the CLI, or both. The desktop app is a menu bar app on macOS, and a tray app on Windows and Linux. The CLI is one binary, `ppm`, with a terminal UI, for servers, VMs and containers.

| | Desktop app | CLI |
|---|---|---|
| macOS | `brew install --cask greenfield-inc/tap/port-process-manager` | `brew install greenfield-inc/tap/ppm` |
| Windows | `winget install Greenfield.PortProcessManager` | `irm https://github.com/greenfield-inc/port-process-manager/releases/latest/download/install.ps1 \| iex` |
| Linux | `.deb`, `.rpm` or `.AppImage` from [Releases](https://github.com/greenfield-inc/port-process-manager/releases/latest) | `curl -fsSL https://github.com/greenfield-inc/port-process-manager/releases/latest/download/install.sh \| sh` |

See [Install](../README.md#install) for npm, PyPI and Cargo.

## The desktop app

1. Open Port Process Manager. On macOS, its menu bar icon shows how many servers are running.
2. Click the icon, or press <kbd>⌥</kbd> <kbd>⌘</kbd> <kbd>P</kbd> (<kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd> on Windows and Linux) from any app. A small window, the popover, opens under the icon.
3. The popover lists every dev server on this computer. The top shows the memory your servers use. Hover it to see other apps and what's free.
4. Click a server for its details: memory and CPU charts for the last 10 minutes, its process tree, project, branch, and the coding agent session that started it.
5. Click **Clean up** to see the servers you probably don't need, and stop them.

On many Linux desktops a left click opens the tray menu. Pick **Open Port Process Manager** at the top.

### Keyboard

| Keys | Action |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> | Select a server |
| <kbd>Enter</kbd> | Open its details |
| <kbd>Esc</kbd> or <kbd>←</kbd> | Go back |
| <kbd>⌘</kbd> <kbd>O</kbd> | Open it in your browser |
| <kbd>⌘</kbd> <kbd>R</kbd> | Restart it |
| <kbd>⌘</kbd> <kbd>⌫</kbd> | Stop it |

On Windows and Linux, use <kbd>Ctrl</kbd> instead of <kbd>⌘</kbd>. In Clean up, <kbd>Enter</kbd> or <kbd>Space</kbd> selects a server.

### Protected servers

Databases such as `postgres` and `redis-server` are protected. Clean up and auto-kill skip them, and Stop and Restart ask you to confirm first. Edit the list in [Settings → Clean up](settings.md#protected-processes).

## The terminal

Run `ppm` for the terminal UI. It shows the same list, with the same details and actions.

| Keys | Action |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> or <kbd>j</kbd> <kbd>k</kbd> | Select a server |
| <kbd>Enter</kbd> | Open its details |
| <kbd>o</kbd> | Open it in your browser |
| <kbd>v</kbd> | Open its Vercel preview, when [previews are on](settings.md#general) |
| <kbd>s</kbd> | Stop it |
| <kbd>r</kbd> | Restart it |
| <kbd>c</kbd> | Clean up |
| <kbd>Tab</kbd> or <kbd>m</kbd> | Switch machines |
| <kbd>?</kbd> | All keys |
| <kbd>q</kbd> | Quit |

For scripts, use the commands:

```bash
ppm list                  # the servers, once
ppm list --json           # the same, as JSON
ppm watch --jsonl         # a JSON snapshot on every change
ppm stop 5173             # stop the server on port 5173
ppm clean --yes           # stop what Clean up suggests, without asking
```

See the [CLI reference](cli.md) for every command.

## Next

- [Watch other machines](remote-machines.md) over SSH, Docker, Kubernetes or WSL.
- [Set alerts, clean up and auto-kill](settings.md).
- [Fix a problem](troubleshooting.md).
