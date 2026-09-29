#!/usr/bin/env bash
# Installs ppm the ways the README lists, against a local copy of a release.
#
#   scripts/smoke-install.sh [dist dir]   (default: dist, as scripts/dist.sh writes it)
#
# Serves dist/release over HTTP, then runs install.sh (or install.ps1 on
# Windows), the npm package and the PyPI package through uvx. Each must install
# a ppm that prints the release version. Then it serves a copy whose SHA256SUMS
# is wrong, and each one must refuse to install. Needs node, npm and uv.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$(cd "${1:-$root/dist}" && pwd)"
version="$(node -p "require('$root/packaging/npm/package.json').version")"
work="$(mktemp -d)"
servers=()
trap 'kill "${servers[@]}" 2>/dev/null || true; rm -rf "$work"' EXIT

# Windows tools need Windows paths; elsewhere this is a no-op.
native() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else echo "$1"; fi; }

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
sed -E 's/^[0-9a-f]{64}/0000000000000000000000000000000000000000000000000000000000000000/' \
  "$dist/release/SHA256SUMS" > "$work/tampered/SHA256SUMS"
serve "$dist/release" 18765
serve "$work/tampered" 18766
good=http://127.0.0.1:18765
bad=http://127.0.0.1:18766

npm install --silent --global --prefix "$(native "$work/npm")" "$(native "$dist/packages/port-process-manager-$version.tgz")"
wheel="$(native "$dist/packages/port_process_manager-$version-py3-none-any.whl")"

# Runs one install method with a fresh home, so nothing is cached.
# Usage: install_with <name> <download url> <command...>
install_with() {
  local name="$1" url="$2" home
  shift 2
  home="$work/home-$name-${url##*:}"
  mkdir -p "$home"
  env PPM_DOWNLOAD_URL="$url" PPM_INSTALL_DIR="$(native "$home/bin")" \
    HOME="$home" XDG_CACHE_HOME="$home/.cache" LOCALAPPDATA="$(native "$home")" "$@"
}

if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then
  script=(pwsh -NoProfile -File "$(native "$dist/release/install.ps1")")
  npm_bin="$work/npm/ppm.cmd"
  installed=ppm.exe
else
  script=(sh "$dist/release/install.sh")
  npm_bin="$work/npm/bin/ppm"
  installed=ppm
fi

failures=0
expect_version() {
  local name="$1" output
  shift
  if output="$("$@" 2>&1)" && grep -qF "$version" <<< "$output"; then
    echo "ok    $name: $(tail -n 1 <<< "$output")"
  else
    echo "FAIL  $name printed:"; sed 's/^/      /' <<< "$output"; failures=$((failures + 1))
  fi
}
expect_mismatch() {
  local name="$1" output
  shift
  if output="$("$@" 2>&1)"; then
    echo "FAIL  $name installed despite a wrong checksum:"; sed 's/^/      /' <<< "$output"; failures=$((failures + 1))
  elif grep -q 'checksum mismatch' <<< "$output"; then
    echo "ok    $name refused: $(grep 'checksum mismatch' <<< "$output" | head -n 1)"
  else
    echo "FAIL  $name failed for another reason:"; sed 's/^/      /' <<< "$output"; failures=$((failures + 1))
  fi
}

install_with script "$good" "${script[@]}" > "$work/script.log" 2>&1 || { cat "$work/script.log"; exit 1; }
expect_version "install script" "$work/home-script-18765/bin/$installed" --version
expect_version "npm ppm" install_with npm "$good" "$npm_bin" --version
expect_version "npm port-process-manager" install_with npm2 "$good" "${npm_bin/ppm/port-process-manager}" --version
expect_version "uvx port-process-manager" install_with uvx "$good" uvx --from "$wheel" port-process-manager --version
expect_version "uvx ppm" install_with uvx2 "$good" uvx --from "$wheel" ppm --version

expect_mismatch "install script" install_with script "$bad" "${script[@]}"
expect_mismatch "npm" install_with npm "$bad" "$npm_bin" --version
expect_mismatch "uvx" install_with uvx "$bad" uvx --from "$wheel" port-process-manager --version

[ "$failures" = 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo "all install checks passed"
