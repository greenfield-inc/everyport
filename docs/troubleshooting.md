# Troubleshooting

## Start with `everyport doctor`

`everyport doctor` checks what Everyport can see on this machine and exits with status 1 if a check fails:

```text
$ everyport doctor
everyport 0.1.0 · protocol 1 · macOS aarch64 · 10 cores
✓ Listening ports: 35 found
✓ Processes: 1083 found
✓ Folder and command of your own processes
✓ Memory and CPU of your own processes
✓ Settings folder: /Users/you/Library/Application Support/everyport
```

On Linux it also prints a tip for [blank windows on NVIDIA](#the-window-is-blank-on-nvidia). To check another machine, run it there, for example `ssh devbox everyport doctor`.

## `everyport: command not found`

The install scripts put `everyport` in `~/.local/bin`, which isn't on every `PATH`. Add it in your shell profile, such as `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Over SSH, the shell may skip your profile, so call it by its full path: `ssh devbox '~/.local/bin/everyport doctor'`.

## A server is missing

- Everyport shows ports 3000 to 65535. To change the range, set `min_port` and `max_port` in [`config.toml`](settings.md#the-settings-files).
- A server run by another user or the system shows with fewer details. On macOS it can take up to 10 seconds to appear. See [Other users' servers](#other-users-servers).

## Other users' servers

Without admin rights, Everyport sees full details only for processes your user owns.

| OS | Ports owned by other users or the system |
|---|---|
| Linux | Listed with port, address and owner. Linux shows which process holds the port only to root, so run `sudo everyport` to see it. |
| macOS | Listed with port, owner and process name, read from `nettop`. A new one can take up to 10 seconds to appear. |
| Windows | Listed with port, owner and process name. Without admin rights, the owner can be missing. |

## Stop or restart asks first, or refuses in the terminal

The server is protected: a process in its tree is on the **Never stop** list, such as `postgres`. The app asks you to confirm. In the terminal, `everyport stop` and `everyport restart` refuse it:

```text
everyport: redis-server :6379 is protected. Run `everyport stop 6379 --protected` to stop it anyway.
```

Pass `--protected` to `everyport stop` or `everyport restart` to go ahead, or `--force` to `everyport stop` to kill it. See [Protected processes](settings.md#protected-processes).

## A settings file has an error

The app shows the error at the top of Settings and keeps the last settings it could read. `everyport doctor` prints `Settings:` with the error. Fix or delete the file named in the error. See [The settings files](settings.md#the-settings-files).

## Windows

### Installing from Windows PowerShell 5.1

The install command works in Windows PowerShell 5.1, the one Windows ships with, and in PowerShell 7. Paste it into PowerShell, not Command Prompt:

```powershell
irm https://everyport.dev/install.ps1 | iex
```

To pass options, such as `-Cli` for the CLI only, use the script block form:

```powershell
& ([scriptblock]::Create((irm https://everyport.dev/install.ps1))) -Cli
```

If the download fails with a TLS or connection error, update to a current Windows 10 or 11 build, or install [PowerShell 7](https://aka.ms/powershell) and run the command in `pwsh`.

### SmartScreen asks before the installer runs

Windows builds aren't signed yet, so when you download the `.msi` in a browser and open it, SmartScreen shows **Windows protected your PC**. Click **More info**, then **Run anyway**. The install command above doesn't trigger SmartScreen, since a download through PowerShell isn't marked as coming from the internet.

## Linux

### No tray icon on GNOME

GNOME hides tray icons. Install the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension, then log out and back in. Ubuntu ships it turned on.

Many Linux desktops open the tray menu on a left click. Pick **Open Everyport** at the top of the menu, or press <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd>.

### The window is blank on NVIDIA

WebKitGTK can draw a blank window with NVIDIA drivers. Start the app with the environment variable `WEBKIT_DISABLE_DMABUF_RENDERER=1`. For the AppImage:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./everyport-0.1.0-x86_64.AppImage
```

For the `.deb` or `.rpm`, add `env WEBKIT_DISABLE_DMABUF_RENDERER=1` to the start of the `Exec=` line in the app's `.desktop` file.

## Remote machines

- **Can't connect**: run the machine's command yourself, such as `ssh devbox`, and fix what it prints. The command must run without asking for a password; for SSH, use a key or agent.
- **Everyport isn't installed**: click **Install Everyport…** in Settings → Machines, or install it yourself with any command from [CLI only](../README.md#cli-only). `everyport remote list` shows the everyport on each machine.
- **`everyport serve` exits with `is not a loopback address`**: it listens only on `127.0.0.1` or `::1`. Put a tunnel or proxy in front of it. See [Remote machines](remote-machines.md#connect-through-tailscale-or-a-proxy).
- **A web page can't read `everyport serve`**: start it with `--allow-origin` for the page's origin.
