#!/usr/bin/env bash
# Assembles a release from built artifacts.
#
#   scripts/dist.sh <artifacts dir>
#
# The artifacts dir holds the ppm binaries (ppm-<target>) and any desktop
# bundles. Writes:
#   dist/release   files for the GitHub release, with SHA256SUMS
#   dist/packages  npm tarball, Python wheel and sdist (both carrying
#                  SHA256SUMS), Homebrew formula and cask, winget manifests
#
# On a tag build (GITHUB_REF_TYPE=tag) the tag must match the version and every
# packaging file must render.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
artifacts="$(cd "$1" && pwd)"
release="$root/dist/release"
packages="$root/dist/packages"

version="$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" |
  jq -r '.packages[] | select(.name == "port-process-manager") | .version')"
check_version() {
  [ "$2" = "$version" ] || { echo "$1 has version $2, Cargo.toml has $version" >&2; exit 1; }
}
check_version apps/desktop/src-tauri/tauri.conf.json "$(jq -r .version "$root/apps/desktop/src-tauri/tauri.conf.json")"
check_version packaging/npm/package.json "$(jq -r .version "$root/packaging/npm/package.json")"
check_version packaging/pypi/pyproject.toml "$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/packaging/pypi/pyproject.toml")"

strict=false
if [ "${GITHUB_REF_TYPE:-}" = tag ]; then
  strict=true
  [ "$GITHUB_REF_NAME" = "v$version" ] || { echo "tag $GITHUB_REF_NAME does not match version $version" >&2; exit 1; }
fi

rm -rf "$release" "$packages"
mkdir -p "$release" "$packages"
cp "$artifacts"/* "$root/scripts/install.sh" "$root/scripts/install.ps1" "$release/"
(cd "$release" && sha256sum -- * > SHA256SUMS)
echo "== dist/release (version $version)"
cat "$release/SHA256SUMS"

# Fill @VERSION@ and @SHA256:<release file>@ in each packaging template.
render() {
  local template="$1" text name hash
  text="$(sed "s/@VERSION@/$version/g" "$template")"
  while read -r hash name; do
    text="${text//@SHA256:$name@/$hash}"
  done < "$release/SHA256SUMS"
  if grep -q '@SHA256:' <<< "$text"; then
    echo "skipped $(basename "$template"): missing $(grep -o '@SHA256:[^@]*@' <<< "$text" | cut -d : -f 2 | tr -d @ | xargs)" >&2
    $strict && exit 1
    return 0
  fi
  printf '%s\n' "$text" > "$packages/$(basename "$template")"
}
for template in "$root"/packaging/homebrew/*.rb "$root"/packaging/winget/*.yaml; do
  render "$template"
done

# The npm and PyPI packages carry SHA256SUMS, so they verify the binary
# without trusting a checksum from the same server.
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp -R "$root/packaging/npm" "$root/packaging/pypi" "$stage/"
cp "$release/SHA256SUMS" "$stage/npm/"
cp "$release/SHA256SUMS" "$stage/pypi/src/port_process_manager/"
npm pack --silent "$stage/npm" --pack-destination "$packages" > /dev/null
uv build --quiet "$stage/pypi" --out-dir "$packages"
echo "== dist/packages"
ls -1 "$packages"
