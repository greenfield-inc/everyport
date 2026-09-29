#!/usr/bin/env bash
# Screenshots and a GIF of the desktop app on macOS, for PR evidence:
# the tray icon (idle, running, attention), the popover in light and dark
# mode, the popover over a full-screen app, on a second display when one is
# attached, the alert card, an open/close GIF, and other machines: the
# switcher, the install question, installing, a remote server list, a
# discovered host, and a failed connection.
#
# The terminal app running this needs Screen Recording and Accessibility, and
# Docker for the remote machine. It starts throwaway servers on ports
# 39101-39106 here and 39301-39302 in a container it creates, and stops only
# those, by PID or by removing the container. It never stops, restarts or
# cleans anything through ppm. It switches the system between light and dark
# mode, and puts it back when it exits.
#
# The app runs with a scratch HOME, so its machines.toml (a Docker machine,
# and an ssh host to discover) never touches yours. ppm for the container is
# built in Docker and installed from PPM_BINARY_DIR, since no release has it.
#
#   apps/desktop/scripts/qa-visual.sh [app binary] [output folder]
#
# The app binary defaults to the release bundle from
# `pnpm --filter @ppm/desktop tauri build --bundles app`.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
app="${1:-$root/target.noindex/release/bundle/macos/Port Process Manager.app/Contents/MacOS/ppm-desktop}"
out="${2:-$root/qa-visual.noindex/$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out"
tools="$out/.tools"
mkdir -p "$tools"

pids=()
box=ppm-qa-box
dark_before="$(osascript -e 'tell application "System Events" to tell appearance preferences to get dark mode')"
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  docker rm -f "$box" >/dev/null 2>&1 || true
  osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $dark_before" >/dev/null
}
trap cleanup EXIT

fail() { echo "qa-visual: $*" >&2; exit 1; }

# ---------------------------------------------------------------- preflight
[[ -x "$app" ]] || fail "no app at $app. Build it with: pnpm --filter @ppm/desktop tauri build --bundles app"
screencapture -x "$tools/probe.png" 2>/dev/null || fail "Screen Recording is off for this terminal app (System Settings → Privacy & Security)."
osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' >/dev/null 2>&1 \
  || fail "Accessibility is off for this terminal app (System Settings → Privacy & Security)."
command -v ffmpeg >/dev/null || fail "the GIF needs ffmpeg: brew install ffmpeg"
docker info >/dev/null 2>&1 || fail "the remote machine needs Docker running"

# A container as a remote machine, with its own servers and no ppm yet.
docker rm -f "$box" >/dev/null 2>&1 || true
docker run -d --name "$box" python:3.12-slim sleep infinity >/dev/null
for port in 39301 39302; do docker exec -d -w /tmp "$box" python3 -m http.server "$port"; done
arch="$(docker exec "$box" uname -m)"
echo "qa-visual: building ppm for Linux $arch in Docker"
docker run --rm -v "$root":/src:ro -v "$out/.linux-target":/target -w /src -e CARGO_TARGET_DIR=/target \
  rust:1-bookworm cargo build --release -q -p port-process-manager
mkdir -p "$tools/bin"
# Named as the release file the app asks for. The glibc build runs in this Debian image.
cp "$out/.linux-target/release/ppm" "$tools/bin/ppm-$arch-unknown-linux-musl"

home="$tools/home"
config="$home/Library/Application Support/port-process-manager"
mkdir -p "$config" "$home/.ssh"
printf '[[machine]]\nname = "%s"\ncommand = ["docker", "exec", "-i", "%s"]\n' "$box" "$box" >"$config/machines.toml"
# Discovered, never reachable: ssh reads the real config, where it doesn't exist.
printf 'Host qa-devbox\n' >"$home/.ssh/config"

# Posts a real mouse click, and lists the screens, through CoreGraphics.
cat >"$tools/mouse.swift" <<'SWIFT'
import AppKit
let args = CommandLine.arguments
if args[1] == "screens" {
    // Top-left origin points, as screencapture -R takes them.
    let top = NSScreen.screens[0].frame.maxY
    for s in NSScreen.screens {
        let f = s.frame
        print(Int(f.minX), Int(top - f.maxY), Int(f.width), Int(f.height))
    }
    exit(0)
}
let point = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
for type in [CGEventType.leftMouseDown, .leftMouseUp] {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
    usleep(40_000)
}
SWIFT
swiftc -O "$tools/mouse.swift" -o "$tools/mouse" 2>/dev/null
mouse() { "$tools/mouse" "$@"; }

set_dark() { osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $1" >/dev/null; sleep 1.5; }

# The tray icon's rect in points: x y width height.
tray_rect() {
  osascript -e "tell application \"System Events\" to tell (first process whose unix id is $app_pid) to get {position, size} of menu bar item 1 of menu bar 2" \
    | tr -d ' ' | tr ',' ' '
}

click_tray() {
  read -r x y w h < <(tray_rect)
  mouse click $((x + w / 2)) $((y + h / 2))
  sleep 0.6
}

# A region around the tray icon and the popover below it.
shot_popover() {
  read -r x y w h < <(tray_rect)
  local left=$((x + w / 2 - 240))
  screencapture -x -R "$((left < 0 ? 0 : left)),0,480,640" "$out/$1.png"
}

shot_tray() {
  read -r x y w h < <(tray_rect)
  screencapture -x -R "$((x - 16)),0,$((w + 32)),$((h + 2))" "$out/$1.png"
}

press_escape() { osascript -e 'tell application "System Events" to key code 53'; sleep 0.4; }

# Clicks the control in the popover whose accessible name starts with $1,
# such as a machine chip ("ppm-qa-box") or a button ("Install ppm").
click_named() {
  osascript - "$app_pid" "$1" <<'APPLESCRIPT' >/dev/null
on run {pid, label}
  tell application "System Events" to tell (first process whose unix id is (pid as integer))
    repeat with w in windows
      repeat with e in (entire contents of w)
        try
          if (name of e starts with label) or (description of e starts with label) or (title of e starts with label) then
            click e
            return
          end if
        end try
      end repeat
    end repeat
  end tell
  error "nothing named " & label
end run
APPLESCRIPT
  sleep 0.8
}

start_server() { # port [extra python]
  local dir
  dir="$(mktemp -d "${TMPDIR:-/tmp}/ppm-qa-$1.XXXX")"
  (cd "$dir" && exec python3 -c "${2:-}
import http.server, socketserver
socketserver.TCPServer(('127.0.0.1', $1), http.server.SimpleHTTPRequestHandler).serve_forever()") &
  pids+=($!)
}

# ---------------------------------------------------------------- run
DOCKER_CONFIG="${DOCKER_CONFIG:-$HOME/.docker}" HOME="$home" PPM_BINARY_DIR="$tools/bin" "$app" 2>"$out/app.log" &
app_pid=$!
pids+=("$app_pid")
for _ in $(seq 1 40); do tray_rect >/dev/null 2>&1 && break; sleep 0.25; done
sleep 3

for mode in dark light; do
  set_dark "$([[ $mode == dark ]] && echo true || echo false)"
  shot_tray "tray-idle-$mode"
done

for port in 39101 39102 39103 39104 39105; do start_server "$port"; done
sleep 5 # two scans

for mode in dark light; do
  set_dark "$([[ $mode == dark ]] && echo true || echo false)"
  shot_tray "tray-running-$mode"
  click_tray
  shot_popover "popover-$mode"
  press_escape
done

# Other machines: the switcher, the install question, then the container's
# servers once ppm is on it, and a discovered host that can't be reached.
for mode in dark light; do
  set_dark "$([[ $mode == dark ]] && echo true || echo false)"
  click_tray
  shot_popover "machines-switcher-$mode"
  click_named "$box"
  shot_popover "machine-install-question-$mode"
  click_named "qa-devbox"
  shot_popover "machine-discovered-$mode"
  press_escape
done
set_dark true
click_tray
click_named "$box"
click_named "Install ppm"
shot_popover "machine-installing-dark"
for _ in $(seq 1 60); do grep -q "machine $box: connected" "$out/app.log" && break; sleep 1; done
sleep 3
for mode in dark light; do
  set_dark "$([[ $mode == dark ]] && echo true || echo false)"
  [[ $mode == light ]] && { click_tray; click_named "$box"; }
  shot_popover "machine-remote-list-$mode"
  press_escape
  shot_tray "tray-all-machines-$mode"
done
set_dark true
click_tray
click_named "qa-devbox"
click_named "Connect"
sleep 3
shot_popover "machine-error-dark"
press_escape

# Open and close, recorded.
read -r x y w h < <(tray_rect)
left=$((x + w / 2 - 240)); left=$((left < 0 ? 0 : left))
screencapture -x -v -V 5 -R "$left,0,480,640" "$out/open-close.mov" &
sleep 1
click_tray; sleep 1.2; click_tray; sleep 0.8; click_tray; sleep 1.2; press_escape
wait $!
ffmpeg -loglevel error -y -i "$out/open-close.mov" \
  -vf "fps=30,scale=480:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse" "$out/open-close.gif"

# Over a full-screen app, opened with the global shortcut.
osascript -e 'tell application "TextEdit" to make new document' -e 'tell application "TextEdit" to activate' >/dev/null
sleep 1
osascript -e 'tell application "System Events" to keystroke "f" using {control down, command down}'
sleep 3
osascript -e 'tell application "System Events" to keystroke "p" using {option down, command down}'
sleep 0.8
screencapture -x -m "$out/popover-over-fullscreen.png"
press_escape
osascript -e 'tell application "System Events" to keystroke "f" using {control down, command down}'
sleep 2
osascript -e 'tell application "TextEdit" to close every document saving no' >/dev/null

# The second display, clicking its copy of the tray icon (same distance from its right edge).
screens="$(mouse screens)"
if [[ "$(wc -l <<<"$screens")" -gt 1 ]]; then
  read -r mx _ mw _ < <(sed -n 1p <<<"$screens")
  read -r sx sy sw _ < <(sed -n 2p <<<"$screens")
  read -r x y w h < <(tray_rect)
  cx=$((sx + sw - (mx + mw - x - w / 2)))
  mouse click "$cx" "$((sy + h / 2))"
  sleep 0.8
  screencapture -x -R "$((cx - 240)),$sy,480,640" "$out/popover-second-display.png"
  press_escape
else
  echo "qa-visual: one display attached, skipping the second-display shot"
fi

# Attention: a server over the 2 GB alert threshold, then the alert card.
start_server 39106 "ballast = bytearray(2200 * 1024 * 1024)"
sleep 8
shot_tray "tray-attention-dark"
read -r mx my mw _ < <(sed -n 1p <<<"$screens")
screencapture -x -R "$((mx + mw - 420)),$my,420,180" "$out/alert-card-dark.png"

echo "qa-visual: wrote $(ls "$out"/*.png "$out"/*.gif | wc -l | tr -d ' ') files to $out"
