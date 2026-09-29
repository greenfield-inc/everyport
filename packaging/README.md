# Packaging

Files that package and publish `ppm` and the desktop app.

| Path | What |
|---|---|
| `npm/` | npm package `port-process-manager`: runs the release binary, downloaded on first run and checked against the `SHA256SUMS` packed into the package |
| `pypi/` | PyPI package `port-process-manager`, the same for `uvx` and `pipx` |
| `homebrew/ppm.rb` | Formula for `greenfield-inc/homebrew-tap` |
| `homebrew/port-process-manager.rb` | Cask for the same tap |
| `winget/` | winget manifests for `Greenfield.PortProcessManager` |
| `../scripts/install.sh`, `../scripts/install.ps1` | CLI install scripts, attached to each release |

The Homebrew and winget files are templates. `scripts/dist.sh` fills in `@VERSION@` and each `@SHA256:<release file>@` from the release's `SHA256SUMS`, and packs that `SHA256SUMS` into the npm and PyPI packages.

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
- `ci.yml` runs `pnpm check`, `scripts/publish-crates.sh --dry-run` and `build.yml` on every PR. It builds the desktop bundles only when `apps/`, `packages/`, the lockfile or `build.yml` change.
- `release.yml` runs `build.yml` with bundles. On a `v*` tag it signs and notarizes the bundles, creates the GitHub release, and publishes to crates.io, npm, PyPI and the Homebrew tap. Run it by hand on a branch for an unsigned dry run that publishes nothing. Run it by hand on a tag with **unsigned** checked for an unsigned test release.

## Secrets

A tag build fails when a signing or notarization secret is missing, unless it was started by hand with **unsigned**. Each publish step logs a notice and skips when its token is missing. Builds that aren't from a tag never sign, so they never read the signing secrets.

| Secret | Used for |
|---|---|
| `CSC_LINK`, `CSC_KEY_PASSWORD` | macOS signing: the Developer ID Application certificate as base64 `.p12`, and its password |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_SPECIFIC_PASSWORD` | macOS notarization |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Windows signing: base64 `.pfx` and its password |
| `CARGO_REGISTRY_TOKEN` | crates.io publish of `port-process-manager` and the workspace crates it depends on |
| `NPM_TOKEN` | npm publish |
| `PYPI_API_TOKEN` | PyPI publish |
| `HOMEBREW_TAP_TOKEN` | Push to `greenfield-inc/homebrew-tap` |

## Cut a release

1. Set the same version in `Cargo.toml` (`workspace.package`, and the `ppm-core` and `ppm-client` entries under `workspace.dependencies`), `apps/desktop/src-tauri/tauri.conf.json`, `packaging/npm/package.json` and `packaging/pypi/pyproject.toml`, then run `cargo check` to update `Cargo.lock`. `scripts/dist.sh` fails when they differ.
2. Merge the version bump to main, pull, then tag that commit: `git tag v0.2.0 && git push origin v0.2.0`. `scripts/dist.sh` fails when the tag doesn't match the version.
3. Submit the winget manifests from the `packages` artifact of the tag's Release run to [winget-pkgs](https://github.com/microsoft/winget-pkgs), for example with `wingetcreate submit`. Submissions are by hand for now, including the first one, which winget reviews manually.

## Try it locally

```bash
cargo build -p port-process-manager --release --target aarch64-apple-darwin
mkdir -p /tmp/artifacts && cp target.noindex/aarch64-apple-darwin/release/ppm /tmp/artifacts/ppm-aarch64-apple-darwin
scripts/dist.sh /tmp/artifacts
scripts/smoke-install.sh
```

With one binary, `dist.sh` prints `skipped` for the Homebrew and winget files that need the others. That is expected.
