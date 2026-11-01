#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Usage: $0 <version>" >&2
  exit 1
fi
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[[ "$VERSION" == "$(jq -r '.version' "$ROOT/src-tauri/tauri.conf.json")" ]] || {
  echo "Version does not match tauri.conf.json" >&2; exit 1;
}
PLATFORMS='{}'
add_artifact() {
  local platform="$1" artifact="$2" signature
  if [[ -f "$artifact" && -f "$artifact.sig" ]]; then
    signature="$(cat "$artifact.sig")"
    [[ -n "$signature" ]] || { echo "Empty signature: $artifact.sig" >&2; exit 1; }
    PLATFORMS="$(jq -c --arg platform "$platform" \
      --arg url "https://github.com/shellrow/netdia/releases/download/v$VERSION/$(basename "$artifact")" \
      --arg signature "$signature" \
      '. + {($platform): {url: $url, signature: $signature}}' <<< "$PLATFORMS")"
  fi
}
add_artifact darwin-aarch64 "$ROOT/build/NetDia.app.tar.gz"
add_artifact linux-x86_64 "$ROOT/build/linux/netdia-v${VERSION}-linux-x64.AppImage"
[[ "$PLATFORMS" != '{}' ]] || { echo "No signed updater artifacts found in build/" >&2; exit 1; }
jq -n --arg version "$VERSION" --arg date "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
  --argjson platforms "$PLATFORMS" \
  '{version: $version, notes: "See GitHub Releases for details.", pub_date: $date, platforms: $platforms}' > "$ROOT/latest.json"
echo "Generated $ROOT/latest.json. Upload the referenced artifacts manually."
