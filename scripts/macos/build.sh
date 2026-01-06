#!/usr/bin/env bash
set -euo pipefail

# -------- config --------
APP_NAME="NetDia"
OUT_DIR_NAME="build"
ARCH="aarch64"
# ------------------------

echo "== Resolve repo root =="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
while [[ "$REPO_ROOT" != "/" ]]; do
  if [[ -d "$REPO_ROOT/src-tauri" ]] || [[ -f "$REPO_ROOT/Cargo.toml" ]]; then
    break
  fi
  REPO_ROOT="$(dirname "$REPO_ROOT")"
done

if [[ "$REPO_ROOT" == "/" ]]; then
  echo "Could not resolve repo root"
  exit 1
fi

echo "RepoRoot: $REPO_ROOT"

OUT_DIR="$REPO_ROOT/$OUT_DIR_NAME"
mkdir -p "$OUT_DIR"

echo "OutputDir: $OUT_DIR"

# -------- build --------
echo ""
echo "== Build (macOS) =="
cargo tauri build

# -------- detect version from dmg --------
DMG_DIR="$REPO_ROOT/src-tauri/target/release/bundle/dmg"

DMG_FILE="$(ls -t "$DMG_DIR"/${APP_NAME}_*_aarch64.dmg 2>/dev/null | head -n 1 || true)"
if [[ -z "$DMG_FILE" ]]; then
  echo "No dmg found in $DMG_DIR"
  exit 1
fi

# NetDia_0.4.0_aarch64.dmg -> 0.4.0
VERSION="$(basename "$DMG_FILE" | sed -E "s/^${APP_NAME}_([0-9]+\.[0-9]+\.[0-9]+)_aarch64\.dmg$/\1/")"

if [[ -z "$VERSION" ]]; then
  echo "Failed to parse version from dmg filename"
  exit 1
fi

echo "Detected version: $VERSION"

# -------- paths --------
APP_SRC="$REPO_ROOT/src-tauri/target/release/bundle/macos/${APP_NAME}.app"
DMG_SRC="$DMG_FILE"

APP_DST="$OUT_DIR/${APP_NAME}.app"
DMG_DST="$OUT_DIR/netdia-v${VERSION}-macos-${ARCH}.dmg"

# -------- copy --------
echo ""
echo "== Copy artifacts =="

if [[ ! -d "$APP_SRC" ]]; then
  echo "App not found: $APP_SRC"
  exit 1
fi

rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"
echo "APP: $APP_DST"

cp -f "$DMG_SRC" "$DMG_DST"
echo "DMG: $DMG_DST"

# -------- hashes --------
echo ""
echo "== SHA256 =="

(
  cd "$REPO_ROOT"
  shasum -a 256 "${OUT_DIR_NAME}/$(basename "$DMG_DST")" \
  > "${OUT_DIR_NAME}/artifacts-sha256.txt"
)

cat "$OUT_DIR/artifacts-sha256.txt"

echo ""
echo "All Done!"
