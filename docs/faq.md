# FAQ

## Is Everyport free?

Yes. It's open source under the [MIT license](../LICENSE), with no account and no paid tier.

## Does it send anything over the internet?

No telemetry and no account. The app goes online only to:

- look up Vercel previews through the GitHub CLI (`gh`), if you turn previews on
- download `everyport` from GitHub Releases when you install it on another machine

## Which servers does it show?

Every process listening on a port from 3000 to 65535. To change the range, set `min_port` and `max_port` in [`config.toml`](settings.md#the-settings-files). They aren't in the Settings window.

## Does it need admin rights?

No. It shows full details for processes your user owns, and less for ports owned by other users or the system. See [Other users' servers](troubleshooting.md#other-users-servers).

## Can it stop my database by accident?

Not by default. Databases such as `postgres` and `redis-server` are on the **Never stop** list. Clean up and auto-kill skip them, and Stop and Restart ask first. In the terminal, `everyport stop` and `everyport restart` refuse them without `--protected`. Edit the list in [Settings → Clean up](settings.md#protected-processes).

## Can I see servers on another machine?

Yes: a devbox over SSH, a Docker container, a Kubernetes pod or a WSL distro. The first time you connect, Everyport asks, then installs itself in `~/.local/bin` there over the same connection. See [Machines](remote-machines.md).

## Do I need the desktop app?

No. The `everyport` CLI has the same list in a terminal UI, and `everyport list --json` for scripts. To install only the CLI, run `curl -fsSL https://everyport.dev/install.sh | sh -s -- --cli`. For Windows and other ways, see [Getting started](getting-started.md#install).

## How do I update?

Run the install command again. It installs the latest release over the old one. If you installed with a package manager, update with it: `brew upgrade`, or `winget upgrade Dcouple.Everyport`.

## How does it relate to WhatThePort?

Everyport began as a cross-platform port of [WhatThePort](https://github.com/tomjohndesign/what-the-port), a macOS app by Tomjohn Design. It adds Windows and Linux, remote machines, the CLI and a JSON protocol.

## Can I build my own tool on it?

Yes. `everyport stdio` and `everyport serve` speak the same JSON protocol, and `everyport watch --jsonl` prints a snapshot on every change. See [the protocol](protocol.md).
