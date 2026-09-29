# Everyport

The [README](README.md) describes the finished product and is the spec. [docs/intent-brief.md](docs/intent-brief.md) holds the decisions, budgets and native-feel rules. [docs/design-spec.md](docs/design-spec.md) covers the look.

## Layout

```text
crates/everyport         the one published crate
  src/lib.rs             library: protocol.rs (the contract), engine, links, platform/{macos,linux,windows}.rs
  src/client/            runs everyport through a connection (sidecar, ssh, docker, kubectl, wsl, custom, or HTTP)
  src/main.rs            the `everyport` binary: CLI, TUI, `stdio` and `serve`
apps/desktop             Tauri app: tray, popover window, native feel; src-tauri is its Rust side
packages/protocol        TypeScript types generated from protocol.rs, EveryportClient interface, fixture
packages/ui              React popover UI; imports only @everyport/protocol and React, never Tauri
docs/design              Paper frames as PNG and exact HTML
reference/               WhatThePort source (MIT, read-only); port behavior from it
```

## How data flows

`Platform` (per OS) → `Engine` (shared) → `Event`s as JSON lines from `everyport stdio` → `everyport::client` in the desktop app → Tauri events → an `EveryportClient` in the web view → `@everyport/ui`.

The desktop app runs `everyport stdio` as a sidecar, even for this machine. A remote machine is the same process started through a command prefix.

## Rules

- The protocol changes only in `crates/everyport/src/protocol.rs`. After changing it, run `pnpm generate` and update `packages/protocol/fixtures/snapshot.json` and `docs/protocol.md` in the same PR.
- Work only in the paths your lane owns (below). If you need a change elsewhere, make the smallest one, and say why in the PR.
- DRY, YAGNI, no code smells. One code path for local and remote. No abstraction without two real users. No dead flags or TODO stubs left in merged code.
- Tests check behavior through public interfaces, with expected values from the spec, the fixture or a worked example. No change-detector or tautological tests. Use a fake `Platform` to test the engine.
- Don't edit `reference/`.
- Human-facing text is plain and concise, with no em dashes.

## Checks

```bash
pnpm install
pnpm check      # typecheck, cargo fmt, clippy -D warnings, cargo test
pnpm dev        # run the desktop app (macOS: needs Xcode command line tools)
cargo run -p everyport -- list
```

CI runs `pnpm check` on macOS, Windows and Linux. A PR merges when CI is green on all three.

## Verify like a user

On this Mac, run what you built and look at it. For UI, screenshot every changed view in light and dark mode, next to its Paper frame (`docs/design/`). For the CLI, paste real output. For performance work, paste measured numbers against the intent brief's budgets. Put the evidence in the PR body.

## Lanes (Wave 1)

| Lane | Owns | Delivers |
|---|---|---|
| `engine` | `crates/everyport/src/engine.rs` and new engine modules | Snapshots from `Platform` facts: process trees, CPU from time deltas, 10-minute history, status, clean up reasons, alerts, stop with ProcRef checks, restart |
| `platform-unix` | `platform/macos.rs`, `platform/linux.rs` | `Platform` on macOS (libproc: physical footprint, cwd, args) and Linux (`/proc`, sockets), with `sysinfo` and `listeners` where they're accurate |
| `platform-windows` | `platform/windows.rs` | `Platform` on Windows, including `wslrelay.exe` detection so WSL ports can be attributed to a distro |
| `links` | `crates/everyport/src/links/` | Project (name, framework, branch, worktree, GitHub), workspace (Conductor, Pane, git worktree), Claude Code and Codex sessions, Vercel preview through `gh` |
| `cli` | `crates/everyport/`, `docs/protocol.md` | Every README command, the TUI, `everyport stdio`, `everyport serve` with token auth, `everyport doctor` |
| `remote` | `crates/everyport/src/client/` | Connections (sidecar, command prefix, HTTP), host discovery (`~/.ssh/config`, Pane remote hosts, manual, WSL distros), install over the connection, port forwarding, reconnect |
| `ui` | `packages/ui/` | All Paper views, pixel-exact, rendered from `fixtureSnapshot`, with Doozy themes, charts, keyboard use and motion |
| `desktop` | `apps/desktop/` | Tray, popover window and every native-feel item in the intent brief, the sidecar wiring, and an `EveryportClient` over Tauri events |
| `release` | `.github/`, `scripts/`, `packaging/` | CI matrix, cross builds of `everyport`, `install.sh`, npm and PyPI wrapper packages, Homebrew formula and cask, Tauri bundles with the sidecar |
