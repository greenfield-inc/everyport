# Troubleshooting

## Start with `ppm doctor`

`ppm doctor` checks what ppm can see on this machine and exits with status 1 if a check fails:

```text
$ ppm doctor
ppm 0.1.0 · protocol 1 · macOS aarch64 · 10 cores
✓ Listening ports: 35 found
✓ Processes: 1083 found
✓ Folder and command of your own processes
✓ Memory and CPU of your own processes
✓ Settings folder: /Users/you/Library/Application Support/port-process-manager
```

On Linux it also prints a tip for [blank windows on NVIDIA](#the-window-is-blank-on-nvidia). To check another machine, run it there, for example `ssh devbox ppm doctor`.

## `ppm: command not found`

The install scripts put `ppm` in `~/.local/bin`, which isn't on every `PATH`. Add it in your shell profile, such as `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Over SSH, the shell may skip your profile, so call it by its full path: `ssh devbox '~/.local/bin/ppm doctor'`.

## A server is missing

- ppm shows ports 3000 to 65535. To change the range, set `min_port` and `max_port` in [`config.toml`](settings.md#the-settings-files).
- A server run by another user or the system shows with fewer details. On macOS it can take up to 10 seconds to appear. See [Other users' servers](#other-users-servers).

## Other users' servers

Without admin rights, ppm sees full details only for processes your user owns.

| OS | Ports owned by other users or the system |
|---|---|
| Linux | Listed with port, address and owner. Linux shows which process holds the port only to root, so run `sudo ppm` to see it. |
| macOS | Listed with port, owner and process name, read from `nettop`. A new one can take up to 10 seconds to appear. |
| Windows | Listed with port, owner and process name. Without admin rights, the owner can be missing. |

## Stop or restart asks first, or refuses in the terminal

The server is protected: a process in its tree is on the **Never stop** list, such as `postgres`. The app asks you to confirm. In the terminal, `ppm stop` and `ppm restart` refuse it:

```text
ppm: redis-server :6379 is protected. Run `ppm stop 6379 --protected` to stop it anyway.
```

Pass `--protected` to `ppm stop` or `ppm restart` to go ahead, or `--force` to `ppm stop` to kill it. See [Protected processes](settings.md#protected-processes).

## A settings file has an error

The app shows the error at the top of Settings and keeps the last settings it could read. `ppm doctor` prints `Settings:` with the error. Fix or delete the file named in the error. See [The settings files](settings.md#the-settings-files).

## Linux

### No tray icon on GNOME

GNOME hides tray icons. Install the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension, then log out and back in. Ubuntu ships it turned on.

Many Linux desktops open the tray menu on a left click. Pick **Open Port Process Manager** at the top of the menu, or press <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>P</kbd>.

### The window is blank on NVIDIA

WebKitGTK can draw a blank window with NVIDIA drivers. Start the app with the environment variable `WEBKIT_DISABLE_DMABUF_RENDERER=1`. For the AppImage:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./port-process-manager-0.1.0-x86_64.AppImage
```

For the `.deb` or `.rpm`, add `env WEBKIT_DISABLE_DMABUF_RENDERER=1` to the start of the `Exec=` line in the app's `.desktop` file.

## Remote machines

- **Can't connect**: run the machine's command yourself, such as `ssh devbox`, and fix what it prints. The command must run without asking for a password; for SSH, use a key or agent.
- **ppm isn't installed**: click **Install ppm…** in Settings → Machines, or install it yourself with any command from [CLI only](../README.md#cli-only). `ppm remote list` shows the ppm on each machine.
- **`ppm serve` exits with `is not a loopback address`**: it listens only on `127.0.0.1` or `::1`. Put a tunnel or proxy in front of it. See [Remote machines](remote-machines.md#connect-through-tailscale-or-a-proxy).
- **A web page can't read `ppm serve`**: start it with `--allow-origin` for the page's origin.
