# Getting started

## Install

Install the desktop app, the CLI, or both. The desktop app is a menu bar app on macOS, and a tray app on Windows and Linux. The CLI is one binary, `everyport`, with a terminal UI, for servers, VMs and containers.

To install both with one command:

```bash
curl -fsSL https://everyport.dev/install.sh | sh          # macOS and Linux
irm https://everyport.dev/install.ps1 | iex               # Windows (PowerShell)
```

Or install each one separately:

| | Desktop app | CLI |
|---|---|---|
| macOS | `brew install --cask greenfield-inc/tap/everyport` | `brew install greenfield-inc/tap/everyport` |
| Windows | `.msi` or `-setup.exe` from [Releases](https://github.com/greenfield-inc/everyport/releases/latest) | `irm https://github.com/greenfield-inc/everyport/releases/latest/download/install.ps1 \| iex` |
| Linux | `.deb`, `.rpm` or `.AppImage` from [Releases](https://github.com/greenfield-inc/everyport/releases/latest) | `curl -fsSL https://github.com/greenfield-inc/everyport/releases/latest/download/install.sh \| sh` |

See [CLI only](../README.md#cli-only) for npm.

## The desktop app

1. Open Everyport. On macOS, its menu bar icon shows how many servers are running.
2. Click the icon, or press <kbd>⌥</kbd> <kbd>⌘</kbd> <kbd>P</kbd> (<kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd> on Windows and Linux) from any app. A small window, the popover, opens under the icon.
3. The popover lists every dev server on this computer. The top shows the memory your servers use. Hover it to see other apps and what's free.
4. Click a server for its details: memory and CPU charts for the last 10 minutes, its process tree, project, branch, and the coding agent session that started it.
5. Click **Clean up** to see the servers you probably don't need, and stop them.

On many Linux desktops a left click opens the tray menu. Pick **Open Everyport** at the top.

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

Run `everyport` for the terminal UI. It shows the same list, with the same details and actions.

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
everyport list                  # the servers, once
everyport list --json           # the same, as JSON
everyport watch --jsonl         # a JSON snapshot on every change
everyport stop 5173             # stop the server on port 5173
everyport clean --yes           # stop what Clean up suggests, without asking
```

See the [CLI reference](cli.md) for every command.

## Updating

The desktop app checks GitHub for a newer release when it starts and every 12 hours, or when you pick **Check for Updates…** in the tray menu. The check sends one request to GitHub and nothing else. When there is a newer release:

- the popover shows an **Update to 0.1.3** button at the bottom
- the tray menu shows **Update to Everyport 0.1.3…**
- a notification appears, once for each new version

Click any of these to update. A terminal opens and runs the install command. It downloads and verifies the new version, quits Everyport, replaces it and opens the new one. In some cases it works differently:

- **No terminal opens:** **Settings → General** shows the command to paste in a terminal, with a **Copy** button.
- **Installed with Homebrew:** the terminal runs `brew upgrade --cask everyport`.
- **Installed from a `.deb`, `.rpm` or `.msi`:** Update opens the release page. Download and install the new package from there.

**Skip This Version**, in the notification or in **Settings → General**, hides that version until a newer one comes out. To stop checking, turn off **Check for updates automatically** in **Settings → General**.

For the CLI, run `everyport update`. If you installed the CLI with Homebrew, cargo, npm or PyPI, it prints the command to update it that way. When run in a terminal, `everyport --version` also tells you if a newer release is out. It checks at most once a day. Set `EVERYPORT_NO_UPDATE_CHECK=1` to turn it off.

## Next

- [Watch other machines](remote-machines.md) over SSH, Docker, Kubernetes or WSL.
- [Set alerts, clean up and auto-kill](settings.md).
- [Fix a problem](troubleshooting.md).
