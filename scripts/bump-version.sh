#!/usr/bin/env bash
# Sets the release version. Cargo.toml holds it; the desktop app, npm and PyPI
# packages read it from there.
#
#   scripts/bump-version.sh 0.2.0
set -euo pipefail

version="${1:-}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "usage: $0 <x.y.z>" >&2; exit 1; }

cd "$(dirname "$0")/.."
sed -E -i.bak "s/^version = \"[^\"]*\"/version = \"$version\"/" Cargo.toml
rm Cargo.toml.bak
cargo update --workspace --quiet
cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | "\(.name) \(.version)"'
grep -E '^version =' Cargo.toml
