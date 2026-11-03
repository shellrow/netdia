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
[[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]] || { echo "This script requires Apple Silicon macOS"; exit 1; }
cd "$REPO_ROOT"
cargo tauri build

# -------- detect version from dmg --------
DMG_DIR="$REPO_ROOT/src-tauri/target/release/bundle/dmg"

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
DMG_FILE="$DMG_DIR/${APP_NAME}_${VERSION}_aarch64.dmg"
[[ -f "$DMG_FILE" ]] || { echo "DMG not found: $DMG_FILE"; exit 1; }

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
cp "$REPO_ROOT/src-tauri/target/release/bundle/macos/NetDia.app.tar.gz" "$OUT_DIR/"
cp "$REPO_ROOT/src-tauri/target/release/bundle/macos/NetDia.app.tar.gz.sig" "$OUT_DIR/"

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
