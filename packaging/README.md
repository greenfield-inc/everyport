# Packaging

Files that package and publish `ppm` and the desktop app.

| Path | What |
|---|---|
| `npm/` | npm package `port-process-manager`: runs the release binary, downloaded and checked on first run |
| `pypi/` | PyPI package `port-process-manager`, the same for `uvx` and `pipx` |
| `homebrew/ppm.rb` | Formula for `greenfield-inc/homebrew-tap` |
| `homebrew/port-process-manager.rb` | Cask for the same tap |
| `winget/` | winget manifests for `Greenfield.PortProcessManager` |
| `../scripts/install.sh`, `../scripts/install.ps1` | CLI install scripts, attached to each release |

The Homebrew and winget files are templates. `scripts/dist.sh` fills in `@VERSION@` and each `@SHA256:<release file>@` from the release's `SHA256SUMS`.

## Release files

| File | Built from |
|---|---|
| `ppm-<target>` (`.exe` on Windows) | `cargo build` or `cargo zigbuild` for `x86_64` and `aarch64` on `apple-darwin`, `unknown-linux-musl` and `pc-windows-msvc` |
| `port-process-manager-<version>-<arch>.dmg` | Tauri, `aarch64` and `x86_64` |
| `port-process-manager-<version>-x86_64.{msi,deb,rpm,AppImage}` | Tauri |
| `install.sh`, `install.ps1` | `scripts/` |
| `SHA256SUMS` | every file above |

## Workflows

- `build.yml` builds all of it, assembles it with `scripts/dist.sh`, and runs `scripts/smoke-install.sh` on macOS, Windows and Linux. It never publishes.
- `ci.yml` runs `pnpm check` and `build.yml` on every PR. It builds the desktop bundles only when `apps/`, `packages/`, the lockfile or `build.yml` change.
- `release.yml` runs `build.yml` with bundles. On a `v*` tag it then creates the GitHub release and publishes to npm, PyPI and the Homebrew tap. Run it by hand on a branch for a dry run that publishes nothing.

## Secrets

Each step runs when its secrets are set, and logs a notice and skips when they aren't.

| Secret | Used for |
|---|---|
| `CSC_LINK`, `CSC_KEY_PASSWORD` | macOS signing: the Developer ID Application certificate as base64 `.p12`, and its password |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_SPECIFIC_PASSWORD` | macOS notarization |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Windows signing: base64 `.pfx` and its password |
| `NPM_TOKEN` | npm publish |
| `PYPI_API_TOKEN` | PyPI publish |
| `HOMEBREW_TAP_TOKEN` | Push to `greenfield-inc/homebrew-tap` |

## Cut a release

1. Set the same version in `Cargo.toml` (`workspace.package`), `apps/desktop/src-tauri/tauri.conf.json`, `packaging/npm/package.json` and `packaging/pypi/pyproject.toml`, then run `cargo check` to update `Cargo.lock`. `scripts/dist.sh` fails when they differ.
2. Merge the version bump to main, pull, then tag that commit: `git tag v0.2.0 && git push origin v0.2.0`. `scripts/dist.sh` fails when the tag doesn't match the version.
3. Submit the winget manifests from the `packages` artifact of the tag's Release run to [winget-pkgs](https://github.com/microsoft/winget-pkgs), for example with `wingetcreate submit`.

## Try it locally

```bash
cargo build -p port-process-manager --release --target aarch64-apple-darwin
mkdir -p /tmp/artifacts && cp target.noindex/aarch64-apple-darwin/release/ppm /tmp/artifacts/ppm-aarch64-apple-darwin
scripts/dist.sh /tmp/artifacts
scripts/smoke-install.sh
```

With one binary, `dist.sh` prints `skipped` for the Homebrew and winget files that need the others. That is expected.
