#!/usr/bin/env bash
set -euo pipefail

# -------- Config --------
APP_NAME="netdia"
ARCH="x64"
PLATFORM="linux"

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
DIST_DIR="$ROOT_DIR/build/linux"
TAURI_BUNDLE_DIR="$ROOT_DIR/src-tauri/target/release/bundle"

# Read version from tauri.conf.json
VERSION="$(jq -r '.version' "$ROOT_DIR/src-tauri/tauri.conf.json")"

# -------- Env checks --------
command -v jq >/dev/null || { echo "jq is required"; exit 1; }
command -v cargo >/dev/null || { echo "cargo is required"; exit 1; }

: "${SIGN:?SIGN=1 is required}"
: "${SIGN_KEY:?SIGN_KEY (GPG key id) is required}"
: "${APPIMAGETOOL_SIGN_PASSPHRASE:?APPIMAGETOOL_SIGN_PASSPHRASE is required}"

# -------- Build --------
echo "==> Building NetDia (Linux)"
cd "$ROOT_DIR"

cargo tauri build

mkdir -p "$DIST_DIR"

# -------- Rename artifacts --------
echo "==> Renaming artifacts"

# AppImage
APPIMAGE_SRC="$(ls "$TAURI_BUNDLE_DIR/appimage/"*.AppImage | head -n 1)"
APPIMAGE_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.AppImage"
cp "$APPIMAGE_SRC" "$APPIMAGE_DST"
chmod +x "$APPIMAGE_DST"

# deb
DEB_SRC="$(ls "$TAURI_BUNDLE_DIR/deb/"*.deb | head -n 1)"
DEB_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.deb"
cp "$DEB_SRC" "$DEB_DST"

# rpm
RPM_SRC="$(ls "$TAURI_BUNDLE_DIR/rpm/"*.rpm | head -n 1)"
RPM_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.rpm"
cp "$RPM_SRC" "$RPM_DST"

# -------- Verify AppImage signature --------
echo "==> AppImage signature info"
"$APPIMAGE_DST" --appimage-signature || true

# -------- Done --------
echo
echo "All Done!"
echo "Artifacts:"
ls -lh "$DIST_DIR"
