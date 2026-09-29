#!/bin/sh
# Installs the ppm CLI from GitHub Releases into ~/.local/bin.
#
#   curl -fsSL https://github.com/greenfield-inc/port-process-manager/releases/latest/download/install.sh | sh
#
# Environment:
#   PPM_VERSION       release to install, such as 0.2.0 (default: latest)
#   PPM_INSTALL_DIR   where to put ppm (default: ~/.local/bin)
#   PPM_DOWNLOAD_URL  folder that holds the release files, for mirrors and testing (default: the GitHub release)
#   PPM_ALLOW_INSECURE set to 1 to allow a PPM_DOWNLOAD_URL that is not https://, for testing
set -eu

repo="https://github.com/greenfield-inc/port-process-manager"

fail() {
  echo "ppm install: $*" >&2
  exit 1
}

download() {
  if command -v curl >/dev/null 2>&1; then
    case "$1" in
      https://*) curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1" ;;
      *) curl -fsSL --retry 3 -o "$2" "$1" ;;
    esac
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    fail "needs curl or wget"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    fail "needs sha256sum or shasum"
  fi
}

# Everything runs from here, so a download cut short runs nothing.
main() {
  install_dir="${PPM_INSTALL_DIR:-$HOME/.local/bin}"

  case "$(uname -s)" in
    Darwin) os=apple-darwin ;;
    Linux) os=unknown-linux-musl ;;
    MINGW* | MSYS* | CYGWIN*) fail "on Windows, run: irm $repo/releases/latest/download/install.ps1 | iex" ;;
    *) fail "unsupported OS: $(uname -s)" ;;
  esac

  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=aarch64 ;;
    *) fail "unsupported CPU: $(uname -m)" ;;
  esac

  # A shell running under Rosetta reports x86_64 on Apple silicon.
  if [ "$os" = apple-darwin ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
    arch=aarch64
  fi

  asset="ppm-$arch-$os"

  if [ -n "${PPM_DOWNLOAD_URL:-}" ]; then
    base="$PPM_DOWNLOAD_URL"
  elif [ -n "${PPM_VERSION:-}" ]; then
    base="$repo/releases/download/v${PPM_VERSION#v}"
  else
    base="$repo/releases/latest/download"
  fi

  # SHA256SUMS comes from the same place as the binary, so only https protects it.
  case "$base" in
    https://*) ;;
    *) [ "${PPM_ALLOW_INSECURE:-}" = 1 ] || fail "$base is not an https:// URL. For testing, set PPM_ALLOW_INSECURE=1." ;;
  esac

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  echo "Downloading $asset from $base"
  download "$base/$asset" "$tmp/ppm" || fail "could not download $base/$asset"
  download "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download $base/SHA256SUMS"

  expected="$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1 }' "$tmp/SHA256SUMS")"
  [ -n "$expected" ] || fail "SHA256SUMS has no entry for $asset"
  actual="$(sha256 "$tmp/ppm")"
  [ "$actual" = "$expected" ] || fail "checksum mismatch for $asset: expected $expected, got $actual"

  mkdir -p "$install_dir"
  chmod 755 "$tmp/ppm"
  mv "$tmp/ppm" "$install_dir/ppm"
  echo "Installed $("$install_dir/ppm" --version) to $install_dir/ppm"

  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) echo "Add $install_dir to your PATH, for example: export PATH=\"$install_dir:\$PATH\"" ;;
  esac
}

main "$@"
