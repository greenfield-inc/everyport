# Settings and clean up

Open Settings with the gear next to **Clean up** in the popover, or **Settings…** in the tray menu. It has three sections: General, Machines and Clean up. Changes apply as soon as you make them, and apply to this computer. To change the limits on another machine, edit the `config.toml` there.

## General

| Setting | Default | What it does |
|---|---|---|
| Launch at login | Off | Starts the app when you log in |
| Open Everyport | <kbd>⌥</kbd> <kbd>⌘</kbd> <kbd>P</kbd> (<kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd>) | The global shortcut. Click it and press new keys, or <kbd>Esc</kbd> to cancel. On macOS it needs <kbd>⌥</kbd> with <kbd>⌘</kbd> or <kbd>⌃</kbd>, or <kbd>⌘</kbd> and <kbd>⌃</kbd> together. On Windows and Linux it needs <kbd>Alt</kbd>, or <kbd>Win</kbd> and <kbd>Ctrl</kbd> together. **Reset** restores the default. |
| Scan every | 2 seconds | How often Everyport checks ports and processes: 1, 2, 5 or 10 seconds |
| Integrations → Vercel previews | Off | Links each branch to its Vercel preview. It uses the GitHub CLI (`gh`), which goes online. |
| Appearance → Mode | System | System, Light or Dark |
| Appearance → Theme | Doozy Default | The color theme. There are 40. |

**Documentation**, at the bottom of General, opens these docs.

## Machines

The first row is **This computer**. Below it are the machines you've added, each with its status: Not connected, Connecting…, Everyport isn't installed, Installing Everyport…, Connected, or Can't connect. **Remove** forgets a machine.

**Add a machine** takes a name and either a command that runs a program on the machine, such as `ssh devbox`, or an `everyport://` connection code from `everyport serve`. See [Remote machines](remote-machines.md).

**Found on this computer** lists machines the app discovered: hosts in `~/.ssh/config`, your Pane remote hosts and, on Windows, WSL distros. **Add** saves one. WSL distros show in the app without being added.

When a machine has no `everyport`, **Install Everyport…** asks before it copies the matching binary over the same connection and checks its checksum. When the app updates, it updates the copy it installed without asking.

## Clean up

### Alerts

| Setting | Default | Options |
|---|---|---|
| Alert when a server uses more than | 2 GB | 512 MB, 1, 2, 4, 8 or 16 GB |
| Call it a leak when it grows by (within 10 minutes) | 500 MB | 250 MB, 500 MB, 1 GB or 2 GB |

### Suggest stopping servers that

**Clean up** in the popover suggests stopping servers that:

| Reason | Default | Options |
|---|---|---|
| Had their folder or worktree deleted | Always on | |
| Have been idle for (no CPU and no open connections) | 4 hours | 1, 2, 4, 8 or 24 hours |
| Have been running for | 3 days | 1, 3, 7 or 14 days |
| Are leaking memory, by the leak setting above | Always on | |

### Auto-kill

**When servers qualify** decides what happens when a server starts to qualify for clean up:

| Mode | What happens |
|---|---|
| Off (default) | Nothing. The server waits under Clean up. |
| Ask | A notification asks before stopping it. |
| Stop them | The app stops it for you. |

Auto-kill acts only on a server that newly qualifies, so turning it on never stops servers that already qualified. It never stops a leaking server or a protected one. Those stay under Clean up for you to decide.

Auto-kill runs only in the desktop app, for this computer. `everyport watch`, the terminal UI and `everyport stdio` never turn it on by themselves. To stop what Clean up suggests from the terminal, run `everyport clean`. It asks first, unless you pass `--yes`.

### Protected processes

**Never stop** lists processes that Clean up and auto-kill always skip, and that Stop and Restart ask about first. A server is protected when any process in its tree has one of these names. The defaults are `postgres`, `redis-server`, `mongod`, `mysqld` and `mysql`. Click **×** to remove one, or type a name in **Add…** and press <kbd>Enter</kbd>.

In the terminal, `everyport stop` and `everyport restart` refuse a protected server. Pass `--protected` to stop or restart it anyway, or `everyport stop --force` to kill it.

## The settings files

Settings live in one folder:

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/everyport` |
| Windows | `%APPDATA%\everyport` |
| Linux | `~/.config/everyport` |

| File | Holds | Used by |
|---|---|---|
| `config.toml` | Scanning, alerts, clean up, protected processes, auto-kill, Vercel previews | The app and the CLI |
| `app.toml` | Theme, mode and shortcut | The app |
| `machines.toml` | Machines you've added | The app and `everyport remote` |
| `serve-token` | The `everyport serve` token | `everyport serve` |

Every key in `config.toml` is optional. Keys you leave out take their defaults:

```toml
min_port = 3000                  # the lowest port everyport shows (not in Settings)
max_port = 65535                 # the highest port everyport shows (not in Settings)
interval_ms = 2000
alert_memory = 2147483648        # bytes
leak_growth = 524288000          # bytes
idle_after_secs = 14400
long_running_after_secs = 259200
protected = ["postgres", "redis-server", "mongod", "mysqld", "mysql"]
auto_kill = "off"                # "off", "ask" or "act" (Stop them)
vercel_previews = false
```

The app picks up edits to the file right away. The CLI reads it when a command starts. If the file can't be read, the app shows the error in Settings and keeps the last settings that worked, and `everyport doctor` names the problem. On another machine, Everyport uses that machine's own `config.toml`.
