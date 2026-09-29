#!/usr/bin/env bash
# Publishes port-process-manager and the workspace crates it depends on to
# crates.io, in dependency order. Extra arguments go to cargo publish.
#
#   scripts/publish-crates.sh --dry-run
set -euo pipefail

cd "$(dirname "$0")/.."
packages=()
while read -r name; do
  packages+=(-p "$name")
done < <(cargo tree -p port-process-manager -e normal,build --prefix none --format '{p}' | awk '$3 ~ /^\(\// { print $1 }' | sort -u)
echo "Publishing: ${packages[*]}"
cargo publish --locked "${packages[@]}" "$@"
