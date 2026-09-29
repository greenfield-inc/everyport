# Remote machines

ppm shows servers on any machine where it can run a program: a devbox over SSH, a Docker container, a Kubernetes pod or a WSL distro. The app runs `ppm stdio` there through a command you give it, and reads the same protocol it reads from this computer.

## WSL

On Windows, the app adds each installed WSL distro for you. Servers inside WSL show under their distro, with their Linux process tree, and open at `localhost` as usual.

## Add a machine

In the app, open **Settings → Machines**:

- **Found on this computer** lists hosts from `~/.ssh/config`, your Pane remote hosts and WSL distros. Click **Add**.
- **Add a machine** takes a name and a command that runs a program on the machine:

| Connection | Command |
|---|---|
| SSH | `ssh devbox` |
| Docker | `docker exec -i my-container` |
| Kubernetes | `kubectl exec -i my-pod --` |
| WSL | `wsl -d Ubuntu --` |
| Anything else | your own command prefix |

The command must run without asking for a password. For SSH, use a key or an agent.

From the terminal:

```bash
ppm remote add devbox -- ssh devbox
ppm remote list                   # saved and discovered machines, and the ppm on each
ppm remote rm devbox
```

The app and the CLI share the saved machines, in `machines.toml` in the [settings folder](settings.md#the-settings-files).

## Install ppm on the machine

The first time you connect, the app checks the machine's OS and CPU and asks to install the matching `ppm`. It downloads the release from GitHub, copies it over the same connection, and checks its SHA-256 checksum on the machine. It installs to `~/.local/bin/ppm`, or `%LOCALAPPDATA%\ppm\ppm.exe` on Windows. When the app updates, it updates a `ppm` it installed there, without asking. A `ppm` you installed yourself, such as with Homebrew, stays as it is.

On read-only machines, or to install it yourself, use any command from [CLI only](../README.md#cli-only).

## Use a machine from the terminal

```bash
ppm --on devbox                   # terminal UI for devbox
ppm --on devbox list --json
ppm --on devbox stop 5173
ppm --on devbox open 5173         # forwards the port and opens it here
```

`--on` works with `list`, `watch`, `stop`, `restart`, `open`, `clean` and the terminal UI. The machine can be a saved one or any host `ppm remote list` discovers.

The first time, ppm asks before installing itself there, and asks again before updating an older copy. Pass `--yes` to skip the question, as in scripts. Without a terminal to ask in, and without `--yes`, ppm stops with an error when it's missing, and uses an older copy as it is.

In the terminal UI, <kbd>Tab</kbd> or <kbd>m</kbd> switches between this computer and your saved machines.

## Open a remote server

Opening a remote server's URL forwards its port to this computer, then opens it in your browser:

| Connection | How the port is forwarded |
|---|---|
| SSH | `ssh -L` |
| Kubernetes | `kubectl port-forward` |
| WSL | Not needed. WSL servers already open at `localhost`. |
| Docker and other commands | `ppm connect` on the machine relays each connection through the same command |
| `ppm serve` | Not supported. Forward the port yourself. |

`ppm --on devbox open 5173` keeps the forward open until you press <kbd>Ctrl</kbd> <kbd>C</kbd>.

Opening a server's folder, its editor, or resuming its Claude Code or Codex session works for this computer and WSL only.

## Connect through Tailscale or a proxy

When a command connection won't work, such as from a browser or a device that can't run `ssh`, run `ppm serve` on the machine. It speaks the same protocol over HTTP:

```bash
tailscale serve --bg http://127.0.0.1:7767
ppm serve --url https://devbox.tail1234.ts.net
```

`ppm serve` listens only on loopback (`127.0.0.1:7767` by default, change it with `--listen`), so it's reachable only through a tunnel or proxy you set up. Every request needs the token in the connection code it prints. `--url` puts the address clients use into the code. `ppm serve` runs until you stop it, so keep it running, for example in `tmux` or as a service.

Add the machine with the connection code `ppm serve` printed, in **Settings → Machines → Add a machine**, or from the terminal:

```bash
ppm remote add devbox --code ppm://eyJ0b2tlbiI6…   # the whole code
```

The code holds the token, so share it only with people who may stop your servers. To make a new token, delete `serve-token` from the [settings folder](settings.md#the-settings-files) and restart `ppm serve`.

### From a web page

Browsers can't read `ppm serve` responses unless you allow the page's origin. Pass `--allow-origin` once per origin:

```bash
ppm serve --allow-origin https://dash.example.com --allow-origin http://localhost:5173
```

See [the protocol](protocol.md#ppm-serve) for the HTTP API.
