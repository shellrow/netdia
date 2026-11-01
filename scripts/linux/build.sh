#!/usr/bin/env bash
set -euo pipefail

# -------- Config --------
APP_NAME="netdia"
ARCH="x64"
PLATFORM="linux"

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
DIST_DIR="$ROOT_DIR/build/linux"
TAURI_BUNDLE_DIR="$ROOT_DIR/src-tauri/target/release/bundle"

# -------- Env checks --------
command -v jq >/dev/null || { echo "jq is required"; exit 1; }
command -v cargo >/dev/null || { echo "cargo is required"; exit 1; }

VERSION="$(jq -r '.version' "$ROOT_DIR/src-tauri/tauri.conf.json")"
[[ "$(uname -m)" == "x86_64" ]] || { echo "This script requires Linux x86_64"; exit 1; }
[[ "$(uname -s)" == "Linux" ]] || { echo "This script requires Linux"; exit 1; }
[[ "${SIGN:-}" == "1" ]] || { echo "SIGN=1 is required"; exit 1; }
: "${SIGN_KEY:?SIGN_KEY (GPG key id) is required}"
: "${APPIMAGETOOL_SIGN_PASSPHRASE:?APPIMAGETOOL_SIGN_PASSPHRASE is required}"

# -------- Build --------
echo "==> Building NetDia (Linux)"
cd "$ROOT_DIR"

cargo tauri build

mkdir -p "$DIST_DIR"

# -------- Rename artifacts --------
echo "==> Renaming artifacts"

shopt -s nullglob
select_artifact() {
  [[ "$#" == "1" ]] || { echo "Expected one artifact for version $VERSION, found $#" >&2; exit 1; }
  printf '%s' "$1"
}

# AppImage
APPIMAGE_SRC="$(select_artifact "$TAURI_BUNDLE_DIR/appimage/"*"_${VERSION}_amd64.AppImage")"
APPIMAGE_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.AppImage"
cp "$APPIMAGE_SRC" "$APPIMAGE_DST"
chmod +x "$APPIMAGE_DST"
cp "$APPIMAGE_SRC.sig" "$APPIMAGE_DST.sig"

# deb
DEB_SRC="$(select_artifact "$TAURI_BUNDLE_DIR/deb/"*"_${VERSION}_amd64.deb")"
DEB_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.deb"
cp "$DEB_SRC" "$DEB_DST"

# rpm
RPM_SRC="$(select_artifact "$TAURI_BUNDLE_DIR/rpm/"*"-${VERSION}-"*.x86_64.rpm)"
RPM_DST="$DIST_DIR/${APP_NAME}-v${VERSION}-${PLATFORM}-${ARCH}.rpm"
cp "$RPM_SRC" "$RPM_DST"

# -------- Display AppImage signature --------
echo "==> AppImage signature info"
"$APPIMAGE_DST" --appimage-signature || true

# -------- Done --------
echo
echo "All Done!"
echo "Artifacts:"
ls -lh "$DIST_DIR"
