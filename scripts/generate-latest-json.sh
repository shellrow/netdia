#!/usr/bin/env bash
set -euo pipefail

VERSION="$1"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/latest.json"
DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

if [ -z "$VERSION" ]; then
  echo "Usage: $0 <version>"
  exit 1
fi

PLATFORMS=""

# ---------- macOS ----------
MAC_APP="$ROOT/src-tauri/target/release/bundle/macos/NetDia.app.tar.gz"
MAC_SIG="$MAC_APP.sig"

if [ -f "$MAC_APP" ] && [ -f "$MAC_SIG" ]; then
  MAC_SIGNATURE=$(cat "$MAC_SIG")
  PLATFORMS+=$(cat <<EOF
    "darwin-aarch64": {
      "url": "https://github.com/shellrow/netdia/releases/download/v$VERSION/NetDia.app.tar.gz",
      "signature": "$MAC_SIGNATURE"
    },
EOF
)
fi

# ---------- Linux ----------
LINUX_APP="$ROOT/src-tauri/target/release/bundle/appimage/NetDia.AppImage"
LINUX_SIG="$LINUX_APP.sig"

if [ -f "$LINUX_APP" ] && [ -f "$LINUX_SIG" ]; then
  LINUX_SIGNATURE=$(cat "$LINUX_SIG")
  PLATFORMS+=$(cat <<EOF
    "linux-x86_64": {
      "url": "https://github.com/shellrow/netdia/releases/download/v$VERSION/NetDia.AppImage",
      "signature": "$LINUX_SIGNATURE"
    },
EOF
)
fi

PLATFORMS="${PLATFORMS%,}"

if [ -z "$PLATFORMS" ]; then
  echo "Error: No updater artifacts found"
  exit 1
fi

cat <<EOF > "$OUT"
{
  "version": "$VERSION",
  "notes": "See GitHub Releases for details.",
  "pub_date": "$DATE",
  "platforms": {
$PLATFORMS
  }
}
EOF

echo "Done! latest.json generated: $OUT"
