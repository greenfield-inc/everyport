# Packaging

Files that package and publish `everyport` and the desktop app.

| Path | What |
|---|---|
| `npm/` | npm package `everyport`: runs the release binary, downloaded on first run and checked against the `SHA256SUMS` packed into the package |
| `pypi/` | PyPI package `everyport`, the same for `uvx` and `pipx` |
| `homebrew/Formula/everyport.rb` | Formula for `greenfield-inc/homebrew-tap` |
| `homebrew/Casks/everyport.rb` | Cask for the same tap, laid out as the tap lays it out |
| `winget/` | winget manifests for `Dcouple.Everyport` |
| `installer-art/` | Renders the DMG window and Windows installer art. The desktop build runs it before bundling. |
| `../scripts/install.sh`, `../scripts/install.ps1` | CLI install scripts, attached to each release |
| `../scripts/install-app.sh`, `../scripts/install-app.ps1` | Desktop app and CLI install scripts, attached to each release and served by the site as `/install.sh` and `/install.ps1` |

The Homebrew and winget files are templates. `scripts/dist.sh` fills in `@VERSION@` and each `@SHA256:<release file>@` from the release's `SHA256SUMS`, and packs that `SHA256SUMS` into the npm and PyPI packages.

The version lives only in `Cargo.toml` (`workspace.package`). The desktop app reads it from there, and `scripts/dist.sh` stamps it into the npm and PyPI packages.

## Release files

| File | Built from |
|---|---|
| `everyport-<target>` (`.exe` on Windows) | `cargo build` or `cargo zigbuild` for `x86_64` and `aarch64` on `apple-darwin`, `unknown-linux-musl` and `pc-windows-msvc` |
| `everyport-<version>-<arch>.dmg` | Tauri, `aarch64` and `x86_64` |
| `everyport-<version>-x86_64.{msi,deb,rpm,AppImage}` | Tauri |
| `everyport-<version>-x86_64-setup.exe` | Tauri's NSIS installer, per user. `install-app.ps1` uses it because the `.msi` installs per machine and needs admin. |
| `install.sh`, `install.ps1`, `install-app.sh`, `install-app.ps1` | `scripts/` |
| `SHA256SUMS` | every file above |

## Workflows

- `build.yml` builds all of it, assembles it with `scripts/dist.sh`, checks the npm and PyPI packages with `npm publish --dry-run` and `twine check`, and runs `scripts/smoke-install.sh` on macOS, Windows and Linux. It never publishes.
- `ci.yml` runs `pnpm check` and `cargo publish -p everyport --dry-run` on every PR. A PR runs `build.yml` only when it changes `packaging/`, the install scripts, `apps/desktop/src-tauri`, `Cargo.lock` or the CI workflows. Pushes to `main` and manual runs (`gh workflow run CI --ref <branch>`) always run `build.yml`.
- `release.yml` runs `build.yml`. On a `v*` tag it signs and notarizes the bundles, creates the GitHub release, and publishes to crates.io, npm, PyPI and the Homebrew tap. crates.io gets one crate, `everyport`, which holds the library and the CLI. Run it by hand on a branch for an unsigned dry run that publishes nothing. Run it by hand on a tag with **unsigned** checked for an unsigned test release.

## Secrets

A tag build fails when a signing or notarization secret is missing, unless it was started by hand with **unsigned**. To release before there is a Windows certificate, set the `WINDOWS_SIGNING` repository variable to `skip`: tag builds still sign and notarize macOS, and ship an unsigned `.msi` with a warning in the log. Unset or `required`, a tag build fails without the Windows secrets. crates.io, npm and PyPI use trusted publishing, so they need no secret. Each registry's `everyport` package needs a trusted publisher with owner `greenfield-inc`, repository `everyport`, workflow `release.yml` and no environment. A missing one fails only that publish job, after the GitHub release exists. The Homebrew step logs a notice and skips when its token is missing. Builds that aren't from a tag never sign, so they never read the signing secrets.

| Secret | Used for |
|---|---|
| `CSC_LINK`, `CSC_KEY_PASSWORD` | macOS signing: the Developer ID Application certificate as base64 `.p12`, and its password |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_SPECIFIC_PASSWORD` | macOS notarization |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Windows signing: base64 `.pfx` and its password |
| `HOMEBREW_TAP_TOKEN` | Push to `greenfield-inc/homebrew-tap` |

## Cut a release

1. Run `scripts/bump-version.sh 0.2.0`. It sets the version in `Cargo.toml` and `Cargo.lock`.
2. Merge the version bump to main, pull, then tag that commit: `git tag v0.2.0 && git push origin v0.2.0`. `scripts/dist.sh` fails when the tag doesn't match the version.
3. Submit the winget manifests from the `packages` artifact of the tag's Release run to [winget-pkgs](https://github.com/microsoft/winget-pkgs), for example with `wingetcreate submit`. Submissions are by hand for now, including the first one, which winget reviews manually.

## Try it locally

```bash
cargo build -p everyport --release --target aarch64-apple-darwin
mkdir -p /tmp/artifacts && cp target.noindex/aarch64-apple-darwin/release/everyport /tmp/artifacts/everyport-aarch64-apple-darwin
scripts/dist.sh /tmp/artifacts
scripts/smoke-install.sh
```

With one binary, `dist.sh` prints `skipped` for the Homebrew and winget files that need the others. That is expected.
