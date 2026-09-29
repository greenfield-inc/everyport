#!/usr/bin/env bash
# Installs everyport the ways the README lists, against a local copy of a release.
#
#   scripts/smoke-install.sh [dist dir]   (default: dist, as scripts/dist.sh writes it)
#
# Serves dist/release over HTTP, then runs install.sh (or install.ps1 on
# Windows, under both pwsh and Windows PowerShell 5.1), the npm package and the
# PyPI package through uvx. Each must install an everyport that prints the release
# version. When the release has this OS's desktop app, install-app.sh (or
# install-app.ps1) must install it and everyport too. Then it serves a copy whose
# binaries and bundles are altered, and each one must refuse to install. Last,
# against real GitHub, both install scripts must explain a release that doesn't
# exist, and on Windows install-app.ps1 must download a real release file through
# GitHub's redirect. Needs node, npm, uv and network access.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$(cd "${1:-$root/dist}" && pwd)"
work="$(mktemp -d)"
servers=()
trap 'kill "${servers[@]}" 2>/dev/null || true; rm -rf "$work"' EXIT

# Windows tools need Windows paths; elsewhere this is a no-op.
native() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else echo "$1"; fi; }
tarball="$(echo "$dist"/packages/everyport-*.tgz)"
version="${tarball##*/everyport-}"
version="${version%.tgz}"

serve() {
  uv run --quiet --no-project python -m http.server "$2" --bind 127.0.0.1 --directory "$(native "$1")" > "$work/server-$2.log" 2>&1 &
  servers+=($!)
  for _ in $(seq 50); do
    curl -fs "http://127.0.0.1:$2/SHA256SUMS" > /dev/null && return
    sleep 0.2
  done
  echo "server on port $2 did not start" >&2
  exit 1
}

cp -R "$dist/release" "$work/tampered"
for binary in "$work"/tampered/everyport-*; do
  printf 'tampered' >> "$binary"
done
serve "$dist/release" 18765
serve "$work/tampered" 18766
good=http://127.0.0.1:18765
bad=http://127.0.0.1:18766

npm install --silent --global --prefix "$(native "$work/npm")" "$(native "$tarball")"
wheel="$(native "$dist/packages/everyport-$version-py3-none-any.whl")"

# Runs one install method with a fresh home, so nothing is cached.
# Usage: install_with <name> <download url> <command...>
install_with() {
  local name="$1" url="$2" home
  shift 2
  home="$work/home-$name-${url##*:}"
  mkdir -p "$home"
  env EVERYPORT_DOWNLOAD_URL="$url" EVERYPORT_ALLOW_INSECURE=1 EVERYPORT_INSTALL_DIR="$(native "$home/bin")" \
    EVERYPORT_APP_DIR="$(native "$home/app")" HOME="$home" XDG_CACHE_HOME="$home/.cache" \
    XDG_DATA_HOME="$home/.local/share" LOCALAPPDATA="$(native "$home")" "$@"
}

if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then
  runners=("pwsh -NoProfile -File" "powershell -NoProfile -ExecutionPolicy Bypass -File")
  ext="ps1"
  no_open=-NoOpen
  npm_bin="$work/npm"
  cmd=.cmd
  installed=everyport.exe
  app_bundle=(-setup.exe everyport-desktop.exe)
else
  runners=(sh)
  ext="sh"
  no_open=--no-open
  npm_bin="$work/npm/bin"
  cmd=
  installed=everyport
  if [ "$(uname -s)" = Darwin ]; then
    app_bundle=("-$(uname -m | sed 's/arm64/aarch64/').dmg" "Everyport.app/Contents/MacOS/everyport-desktop")
  else
    app_bundle=(.AppImage Everyport.AppImage)
  fi
fi

failures=0
expect_version() {
  local name="$1" output
  shift
  if output="$("$@" 2>&1)" && grep -qF "$version" <<< "$output"; then
    echo "ok    $name: $(tail -n 1 <<< "$output")"
  else
    echo "FAIL  $name printed:"; printf '%s\n' "$output"; failures=$((failures + 1))
  fi
}
expect_mismatch() {
  local name="$1" output
  shift
  if output="$("$@" 2>&1)"; then
    echo "FAIL  $name installed an altered binary:"; printf '%s\n' "$output"; failures=$((failures + 1))
  elif grep -q 'checksum mismatch' <<< "$output"; then
    echo "ok    $name refused: $(grep 'checksum mismatch' <<< "$output" | head -n 1)"
  else
    echo "FAIL  $name failed for another reason:"; printf '%s\n' "$output"; failures=$((failures + 1))
  fi
}

cli_script="$(native "$dist/release/install.$ext")"
app_script="$(native "$dist/release/install-app.$ext")"
for i in "${!runners[@]}"; do
  read -ra runner <<< "${runners[$i]}"
  install_with "script$i" "$good" "${runner[@]}" "$cli_script" > "$work/script$i.log" 2>&1 || { cat "$work/script$i.log"; exit 1; }
  expect_version "install script (${runner[0]})" "$work/home-script$i-18765/bin/$installed" --version
  expect_mismatch "install script (${runner[0]})" install_with "script$i" "$bad" "${runner[@]}" "$cli_script"
done
expect_version "npm everyport" install_with npm "$good" "$npm_bin/everyport$cmd" --version
expect_version "npm everyport" install_with npm2 "$good" "$npm_bin/everyport$cmd" --version
expect_version "uvx everyport" install_with uvx "$good" uvx --from "$wheel" everyport --version
expect_version "uvx everyport" install_with uvx2 "$good" uvx --from "$wheel" everyport --version

expect_mismatch "npm" install_with npm "$bad" "$npm_bin/everyport$cmd" --version
expect_mismatch "uvx" install_with uvx "$bad" uvx --from "$wheel" everyport --version

# The app installers, when this release has this OS's desktop app.
check() {
  local name="$1"
  shift
  if "$@" > /dev/null 2>&1; then echo "ok    $name"; else echo "FAIL  $name"; failures=$((failures + 1)); fi
}
no_quarantine() { ! xattr -r "$1" | grep -q quarantine; }
if compgen -G "$dist/release/everyport-*${app_bundle[0]}" > /dev/null; then
  app_args=("$app_script" "$no_open")
  for i in "${!runners[@]}"; do
    read -ra runner <<< "${runners[$i]}"
    label="app installer (${runner[0]})"
    home="$work/home-app$i-18765"
    install_with "app$i" "$good" "${runner[@]}" "${app_args[@]}" > "$work/app$i.log" 2>&1 || cat "$work/app$i.log"
    check "$label: installs the app" test -e "$home/app/${app_bundle[1]}"
    expect_version "$label: everyport" "$home/bin/$installed" --version
    if [ "$(uname -s)" = Darwin ]; then
      check "$label: no quarantine flag" no_quarantine "$home/app"
      check "$label: codesign --verify" codesign --verify --deep --strict "$home/app/Everyport.app"
    elif [ "$cmd" != .cmd ]; then
      check "$label: icon" test -s "$home/app/icon.png"
      check "$label: menu entry" test -s "$home/.local/share/applications/everyport.desktop"
    fi
    expect_mismatch "$label" install_with "app$i" "$bad" "${runner[@]}" "${app_args[@]}"
    check "$label: installs nothing from an altered release" test ! -e "$work/home-app$i-18766/app" -a ! -e "$work/home-app$i-18766/bin"
  done
else
  echo "skip  app installer: this release has no desktop app for this OS"
fi

# Real GitHub. v0.0.0 is never released, so it answers 404 the way a repo with no release does.
# Usage: expect_failure <name> <pattern the output must match, across lines> <command...>
expect_failure() {
  local name="$1" pattern="$2" output
  shift 2
  if output="$("$@" 2>&1)"; then
    echo "FAIL  $name succeeded:"; printf '%s\n' "$output"; failures=$((failures + 1))
  elif tr '\n' ' ' <<< "$output" | grep -qE "$pattern"; then
    echo "ok    $name:"; printf '%s\n' "$output" | sed 's/^/        /'
  else
    echo "FAIL  $name printed:"; printf '%s\n' "$output"; failures=$((failures + 1))
  fi
}
for i in "${!runners[@]}"; do
  read -ra runner <<< "${runners[$i]}"
  expect_failure "install script (${runner[0]}) with no release" 'no release found at https://github\.com/.*404' \
    env EVERYPORT_VERSION=0.0.0 "${runner[@]}" "$cli_script"
  expect_failure "app installer (${runner[0]}) with no release" 'no release found at https://github\.com/.*404' \
    env EVERYPORT_VERSION=0.0.0 "${runner[@]}" "$app_script" "$no_open"
  # Any public release with a SHA256SUMS works: it downloads through the redirect, then has no Everyport app.
  if [ "$ext" = ps1 ]; then
    expect_failure "app installer (${runner[0]}) downloads through GitHub's redirect" 'no Windows app installer' \
      env EVERYPORT_DOWNLOAD_URL=https://github.com/restic/restic/releases/download/v0.17.3 "${runner[@]}" "$app_script" -NoOpen
  fi
done

[ "$failures" = 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo "all install checks passed"
