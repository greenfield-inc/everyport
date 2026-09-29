#!/bin/sh
# Installs the Port Process Manager desktop app and the ppm CLI from GitHub Releases.
#
#   curl -fsSL https://greenfield-inc.github.io/port-process-manager/install.sh | sh
#   curl -fsSL https://greenfield-inc.github.io/port-process-manager/install.sh | sh -s -- --no-open
#
# macOS: the .app goes into /Applications, or ~/Applications when /Applications
# isn't writable. curl sets no quarantine flag, so the app opens without a
# Gatekeeper prompt. Linux: the AppImage goes into ~/.local/share/port-process-manager
# with a menu entry, or use --deb for the .deb on Debian and Ubuntu.
# Both: ppm goes into ~/.local/bin.
#
# Options:
#   --cli       install only the ppm CLI
#   --deb       Linux: install the .deb with apt (asks for your password)
#   --no-open   don't open the app afterwards
#
# Environment:
#   PPM_VERSION       release to install, such as 0.2.0 (default: latest)
#   PPM_APP_DIR       where the app goes (macOS: the folder for the .app; Linux: the folder for the AppImage)
#   PPM_INSTALL_DIR   where ppm goes (default: ~/.local/bin)
#   PPM_DOWNLOAD_URL  folder that holds the release files, for mirrors and testing (default: the GitHub release)
#   PPM_ALLOW_INSECURE set to 1 to allow a PPM_DOWNLOAD_URL that is not https://, for testing
set -eu

repo="https://github.com/greenfield-inc/port-process-manager"
app_name="Port Process Manager"

fail() {
  echo "Port Process Manager install: $*" >&2
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

cleanup() {
  if [ -n "${mounted:-}" ]; then hdiutil detach -quiet "$mounted" || true; fi
  rm -rf "$tmp"
}

# Prints the release file whose name ends with the given suffix, if there is one.
find_asset() {
  awk -v suffix="$1" '{ name = $2; sub(/^\*/, "", name) } name ~ /^port-process-manager-/ && substr(name, length(name) - length(suffix) + 1) == suffix { print name; exit }' "$tmp/SHA256SUMS"
}

# Downloads a release file into $tmp and checks it against SHA256SUMS.
fetch() {
  echo "Downloading $1"
  download "$base/$1" "$tmp/$1" || fail "could not download $base/$1"
  expected="$(awk -v name="$1" '$2 == name || $2 == "*" name { print $1 }' "$tmp/SHA256SUMS")"
  [ -n "$expected" ] || fail "SHA256SUMS has no entry for $1"
  actual="$(sha256 "$tmp/$1")"
  [ "$actual" = "$expected" ] || fail "checksum mismatch for $1: expected $expected, got $actual"
}

install_macos() {
  app_dir="${PPM_APP_DIR:-/Applications}"
  if [ -z "${PPM_APP_DIR:-}" ] && [ ! -w "$app_dir" ]; then
    app_dir="$HOME/Applications"
  fi
  mkdir -p "$app_dir"
  target="$app_dir/$app_name.app"

  mkdir "$tmp/mnt"
  hdiutil attach -readonly -nobrowse -noautoopen -quiet -mountpoint "$tmp/mnt" "$tmp/$bundle" || fail "could not open $bundle"
  mounted="$tmp/mnt"

  # Quit the copy running from this location the way the Quit menu does, and
  # let it close cleanly. Other copies, such as one in another folder, keep running.
  pids="$(pgrep -f "^$target/Contents/MacOS/" || true)"
  if [ -n "$pids" ]; then
    echo "Quitting $app_name"
    for pid in $pids; do
      osascript -l JavaScript -e "ObjC.import('AppKit'); \$.NSRunningApplication.runningApplicationWithProcessIdentifier($pid).terminate" >/dev/null 2>&1 || true
    done
    i=0
    while pgrep -f "^$target/Contents/MacOS/" >/dev/null 2>&1; do
      i=$((i + 1))
      [ "$i" -le 50 ] || fail "$app_name is still running. Quit it and run this again."
      sleep 0.2
    done
  fi

  # Copy next to the target first, so a failed copy leaves the old app in place.
  rm -rf "$target.new"
  ditto "$tmp/mnt/$app_name.app" "$target.new" || fail "could not copy the app into $app_dir"
  rm -rf "$target"
  mv "$target.new" "$target"
  hdiutil detach -quiet "$mounted" || true
  mounted=
  echo "Installed $app_name to $target"

  if $open_app; then open "$target"; fi
}

install_deb() {
  # apt reads the file as its own user, so let it into the folder.
  chmod 755 "$tmp"
  echo "Installing $bundle with apt"
  sudo apt install -y "$tmp/$bundle" || fail "apt could not install $bundle"
  if $open_app && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    nohup /usr/bin/ppm-desktop >/dev/null 2>&1 &
  fi
}

install_appimage() {
  data="${XDG_DATA_HOME:-$HOME/.local/share}"
  app_dir="${PPM_APP_DIR:-$data/port-process-manager}"
  appimage="$app_dir/Port-Process-Manager.AppImage"
  mkdir -p "$app_dir" "$data/applications"
  chmod 755 "$tmp/$bundle"

  # The AppImage carries its own icon. Extracting needs no FUSE.
  (cd "$tmp" && "./$bundle" --appimage-extract "$app_name.png" >/dev/null 2>&1) || true
  if [ -f "$tmp/squashfs-root/$app_name.png" ]; then
    cp "$tmp/squashfs-root/$app_name.png" "$app_dir/icon.png"
  fi

  mv "$tmp/$bundle" "$appimage"
  cat >"$data/applications/port-process-manager.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$app_name
Comment=See and stop the dev servers on your machines
Exec="$appimage"
Icon=$app_dir/icon.png
Categories=Development;Utility;
Terminal=false
EOF
  echo "Installed $app_name to $appimage"

  if ! { ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; }; then
    echo "AppImages need FUSE 2. On Ubuntu 24.04, run: sudo apt install libfuse2t64"
  fi
  if $open_app && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    nohup "$appimage" >/dev/null 2>&1 &
  fi
}

# Everything runs from here, so a download cut short runs nothing.
main() {
  cli_only=false
  deb=false
  open_app=true
  for arg in "$@"; do
    case "$arg" in
      --cli) cli_only=true ;;
      --deb) deb=true ;;
      --no-open) open_app=false ;;
      *) fail "unknown option $arg (options: --cli, --deb, --no-open)" ;;
    esac
  done

  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW* | MSYS* | CYGWIN*) fail "on Windows, run in PowerShell: irm https://greenfield-inc.github.io/port-process-manager/install.ps1 | iex" ;;
    *) fail "unsupported OS: $(uname -s)" ;;
  esac

  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=aarch64 ;;
    *) fail "unsupported CPU: $(uname -m)" ;;
  esac

  # A shell running under Rosetta reports x86_64 on Apple silicon.
  if [ "$os" = macos ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
    arch=aarch64
  fi

  if [ -n "${PPM_DOWNLOAD_URL:-}" ]; then
    base="$PPM_DOWNLOAD_URL"
  elif [ -n "${PPM_VERSION:-}" ]; then
    base="$repo/releases/download/v${PPM_VERSION#v}"
  else
    base="$repo/releases/latest/download"
  fi

  # SHA256SUMS comes from the same place as the files, so only https protects them.
  case "$base" in
    https://*) ;;
    *) [ "${PPM_ALLOW_INSECURE:-}" = 1 ] || fail "$base is not an https:// URL. For testing, set PPM_ALLOW_INSECURE=1." ;;
  esac

  tmp="$(mktemp -d)"
  trap cleanup EXIT

  download "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download $base/SHA256SUMS"

  bundle=
  if ! $cli_only; then
    if [ "$os" = macos ]; then
      bundle="$(find_asset "-$arch.dmg")"
      [ -n "$bundle" ] || fail "this release has no macOS app for $arch"
    elif $deb; then
      command -v apt >/dev/null 2>&1 || fail "--deb needs apt (Debian or Ubuntu)"
      bundle="$(find_asset "-$arch.deb")"
      [ -n "$bundle" ] || fail "this release has no .deb for $arch"
    else
      bundle="$(find_asset "-$arch.AppImage")"
      if [ -z "$bundle" ]; then
        echo "There is no desktop app for Linux on $arch yet, so this installs only the ppm CLI."
      fi
    fi
  fi

  # Verify everything before installing anything.
  if [ -n "$bundle" ]; then fetch "$bundle"; fi
  fetch install.sh

  PPM_DOWNLOAD_URL="$base" sh "$tmp/install.sh" </dev/null

  [ -n "$bundle" ] || return 0

  case "$os-$bundle" in
    macos-*) install_macos ;;
    *.deb) install_deb ;;
    *) install_appimage ;;
  esac
}

main "$@"
