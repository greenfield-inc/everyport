#!/usr/bin/env bash
# Measures the intent brief's budgets that need no screen: cold start, idle
# memory, idle CPU and scan cost. macOS only.
#
#   scripts/measure-budgets.sh [app]
#
# `app` is the release .app or its binary (default: the one `pnpm --filter
# @ppm/desktop build` writes). Starts 5 throwaway servers on ports 39101-39105,
# launches the app hidden, lets it settle, then measures every process it runs:
# the app, its WebKit processes and the ppm sidecar. Quit the app first.
# Popover open and snapshot-to-UI time need the page on screen, so they're not here.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="${1:-$root/target.noindex/release/bundle/macos/Port Process Manager.app}"
[[ -d "$app" ]] && app="$app/Contents/MacOS/ppm-desktop"
[[ -x "$app" ]] || { echo "No app at $app. Build it with: pnpm --filter @ppm/desktop build" >&2; exit 1; }
settle="${SETTLE:-30}"
window="${WINDOW:-60}"

if pgrep -xq ppm-desktop; then
  echo "Port Process Manager is running. Quit it first; a second launch only opens the popover." >&2
  exit 1
fi

started=()
trap 'kill "${started[@]}" 2>/dev/null || true' EXIT
for port in 39101 39102 39103 39104 39105; do
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$(mktemp -d)" > /dev/null 2>&1 &
  started+=($!)
done

webkit() { pgrep -f 'com\.apple\.WebKit\.' | sort; }
before="$(webkit)"

# Cold start: the app creates the tray icon, then spawns the sidecar.
t0=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
"$app" > /dev/null 2>&1 &
pid=$!
started+=("$pid")
until sidecar="$(pgrep -P "$pid" ppm-sidecar)"; do
  kill -0 "$pid" 2> /dev/null || { echo "The app exited." >&2; exit 1; }
  sleep 0.005
done
t1=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
started+=("$sidecar")

echo "Settling for $settle s with the popover hidden..." >&2
sleep "$settle"
pages="$(comm -13 <(echo "$before") <(webkit))"

name() { ps -o comm= -p "$1" | sed 's|.*/||; s|com\.apple\.WebKit\.||'; }
footprint_mb() {
  footprint -p "$1" 2> /dev/null | awk '/Footprint:/ {
    for (i = 1; i <= NF; i++) if ($i == "Footprint:") { n = $(i + 1); u = $(i + 2) }
    if (u == "KB") n /= 1024; else if (u == "GB") n *= 1024; else if (u == "B") n /= 1048576
    printf "%.1f", n }'
}
# CPU time in seconds, from ps's [[dd-]hh:]mm:ss.cc.
cpu_seconds() {
  ps -o time= -p "$1" | awk '{ n = split($1, p, "[-:]"); s = 0; for (i = 1; i <= n; i++) s = s * 60 + p[i]; print s }'
}

procs=("$pid" $pages "$sidecar")
declare -a cpu_start
for i in "${!procs[@]}"; do cpu_start[$i]=$(cpu_seconds "${procs[$i]}"); done
echo "Measuring CPU over $window s..." >&2
sleep "$window"

printf '\n%-22s %10s %12s\n' "Process" "Memory MB" "CPU % core"
total_mb=0
ui_cpu=0
for i in "${!procs[@]}"; do
  p="${procs[$i]}"
  mb=$(footprint_mb "$p")
  cpu=$(awk -v a="${cpu_start[$i]}" -v b="$(cpu_seconds "$p")" -v w="$window" 'BEGIN { printf "%.3f", (b - a) / w * 100 }')
  if [[ "$p" == "$sidecar" ]]; then
    label="ppm sidecar"
    scan_cpu=$cpu
  else
    if [[ "$p" == "$pid" ]]; then label="app"; else label="$(name "$p")"; fi
    ui_cpu=$(awk -v a="$ui_cpu" -v b="$cpu" 'BEGIN { print a + b }')
  fi
  total_mb=$(awk -v a="$total_mb" -v b="$mb" 'BEGIN { print a + b }')
  printf '%-22s %10s %12s\n' "$label" "$mb" "$cpu"
done

row() { printf '| %s | %s | %s | %s |\n' "$1" "$2" "$3" "$4"; }
verdict() { awk -v v="$1" -v b="$2" 'BEGIN { print (v < b ? "Met" : "**Over**") }'; }
echo
row "Metric" "Budget" "Measured" "Result"
row "---" "---" "---" "---"
row "Cold start to tray (sidecar spawn, just after)" "< 400 ms" "$((t1 - t0)) ms" "$(verdict $((t1 - t0)) 400)"
row "Idle memory, app + WebKit + sidecar" "< 60 MB" "$(printf '%.1f' "$total_mb") MB" "$(verdict "$total_mb" 60)"
row "Idle CPU, popover hidden (app + WebKit)" "< 0.3%" "$(printf '%.3f' "$ui_cpu")%" "$(verdict "$ui_cpu" 0.3)"
row "Scan cost at 2 s (ppm)" "< 1%" "${scan_cpu}%" "$(verdict "$scan_cpu" 1)"
