#!/usr/bin/env bash
# Prints a version's notes from CHANGELOG.md, the body of its "## x.y.z" section.
# Fails when the section is missing or empty. The version, or a v-prefixed tag,
# defaults to Cargo.toml's.
#
#   scripts/release-notes.sh v0.2.0
set -euo pipefail

cd "$(dirname "$0")/.."
version="${1:-$(sed -nE 's/^version = "(.*)"$/\1/p' Cargo.toml | head -1)}"
version="${version#v}"

notes="$(awk -v version="$version" '
  /^## / { in_section = ($2 == version); next }
  in_section
' CHANGELOG.md | sed -e '/./,$!d')"

if ! grep -q '[^[:space:]]' <<< "$notes"; then
  echo "CHANGELOG.md has no notes for $version. Add them under \"## $version\" (scripts/bump-version.sh adds the heading)." >&2
  exit 1
fi
printf '%s\n' "$notes"
