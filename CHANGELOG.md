# Changelog

What changed in each Everyport release, newest first. Also on [everyport.dev/changelog](https://everyport.dev/changelog), with an [RSS feed](https://everyport.dev/feed.xml).

## 0.1.2 - 2026-09-29

### New

- A short welcome on first launch. It shows the servers running now, finds your coding tools, offers to install the `everyport` command, and lists machines you can connect.
- Everyport starts at login on a new install. Turn it off in Settings or on the last welcome step.
- On Windows and Linux, the tray icon shows how many servers are running.
- When a machine can't connect, Everyport shows which step failed and how to fix it, such as turning on SSH or adding your key. `everyport doctor --on <machine>` runs the same check.
- Machines on your Tailscale network show up in the list of machines to add.
- [Docs at everyport.dev/docs](https://everyport.dev/docs), with search.
- The CLI is on crates.io and PyPI: `cargo install everyport`, `uvx everyport` or `pipx install everyport`. `npx everyport` now gets the latest release.

### Improved

- Each server's status is a small slot drawn from the Everyport logo, in place of the two dots.
- Everyport stops retrying a machine when SSH rejects your key or the host key, instead of retrying forever.

### Fixed

- On Windows, the popover opens next to the tray when the Everyport icon is hidden behind the ^ arrow.

## 0.1.1 - 2026-09-29

### Fixed

- The Windows installers work in Windows PowerShell 5.1. The CLI installer failed with "unsupported CPU", and the app installer could pick the wrong build.

## 0.1.0 - 2026-09-29

The first release of Everyport: a menu bar and tray app for macOS, Windows and Linux, and `everyport`, a CLI with a terminal UI.

### New

- Every dev server listening on a port, with its project, git branch or worktree, framework and uptime.
- Memory and CPU for each server's whole process tree, with a 10-minute history chart.
- Alerts when a server's memory crosses your limit or keeps climbing.
- Clean up for servers whose worktree was deleted, or that are idle, long-running or leaking memory. Auto-kill can stop them as they qualify, except leaking ones, which stay for you to decide. Protected processes, such as databases, are skipped, and Stop and Restart ask before touching them.
- The Claude Code or Codex session that started a server, ready to resume in your terminal.
- The Conductor workspace, Pane worktree or git worktree each server runs in, and its Vercel preview.
- Remote machines over SSH, Docker, Kubernetes, WSL or any command, found in `~/.ssh/config`, Pane and WSL, or added by hand. Everyport offers to install itself the first time you connect, and forwards a remote server's port when you open it.
- Every WSL distro on Windows, with the real Linux process behind each port.
- Settings, shared by the app and the CLI in `config.toml`, with 40 color themes in light and dark.
- `everyport list`, `watch`, `stop`, `restart`, `clean`, `doctor` and `--on <machine>`, with JSON output for scripts.
- `everyport stdio` and `everyport serve`, a JSON protocol any client can use.
- One-command install for the app and CLI from everyport.dev, plus Homebrew and npm.
