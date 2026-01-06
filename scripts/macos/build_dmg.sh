#!/usr/bin/env bash
#
# Build a DMG from an built .app, then sign + notarize + staple.
# Also signs + notarizes + staples the .app itself.
#
# Required env:
#   APPLE_SIGN_ID
#   APPLE_NOTARY_KEY_ID
#   APPLE_NOTARY_ISSUER_ID
#   APPLE_NOTARY_KEY_PATH
#
# Usage:
#   ./scripts/macos/build_dmg.sh \
#     --app "/path/to/NetDia.app" \
#     --out "/path/to/netdia-vX.Y.Z-macos-aarch64-signed.dmg"
#
# Optional:
#   --volname "NetDia"
#   --eula "/path/to/EULA.txt"   (plain text or RTF)
#   --skip-applescript          (for headless CI without Finder)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUPPORT_DIR="$SCRIPT_DIR/support"
APPLESCPT_TEMPLATE="$SUPPORT_DIR/template.applescript"
EULA_TEMPLATE_XML="$SUPPORT_DIR/eula-resources-template.xml"

# ----------------------------
# Defaults (Finder layout)
# ----------------------------
VOLNAME_DEFAULT="NetDia"

# Finder window geometry
WINX=200
WINY=120
WINW=560
WINH=360

# Icon and label sizing
ICON_SIZE=128
TEXT_SIZE=14

# Icon positions (Finder coordinates inside the DMG window)
# Put app on left, Applications link on right
APP_ICON_X=140
APP_ICON_Y=170
APPS_LINK_X=420
APPS_LINK_Y=170

# ----------------------------
# Args
# ----------------------------
APP_PATH=""
OUT_DMG=""
VOLNAME="$VOLNAME_DEFAULT"
EULA_FILE=""
SKIP_APPLESCRIPT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)      APP_PATH="$2"; shift 2 ;;
    --out)      OUT_DMG="$2"; shift 2 ;;
    --volname)  VOLNAME="$2"; shift 2 ;;
    --eula)     EULA_FILE="$2"; shift 2 ;;
    --skip-applescript) SKIP_APPLESCRIPT=1; shift ;;
    -h|--help)
      echo "Usage: $0 --app /path/NetDia.app --out /path/out.dmg [--volname NAME] [--eula /path/EULA.txt] [--skip-applescript]"
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$APP_PATH" || -z "$OUT_DMG" ]]; then
  echo "ERROR: --app and --out are required." >&2
  exit 1
fi

# ----------------------------
# Env checks
# ----------------------------
: "${APPLE_SIGN_ID:?Missing env APPLE_SIGN_ID}"
: "${APPLE_NOTARY_KEY_ID:?Missing env APPLE_NOTARY_KEY_ID}"
: "${APPLE_NOTARY_ISSUER_ID:?Missing env APPLE_NOTARY_ISSUER_ID}"
: "${APPLE_NOTARY_KEY_PATH:?Missing env APPLE_NOTARY_KEY_PATH}"

if [[ ! -d "$APP_PATH" || "${APP_PATH: -4}" != ".app" ]]; then
  echo "ERROR: --app must point to a .app directory: $APP_PATH" >&2
  exit 1
fi

if [[ "${OUT_DMG: -4}" != ".dmg" ]]; then
  echo "ERROR: --out must end with .dmg: $OUT_DMG" >&2
  exit 1
fi

if [[ ! -f "$APPLESCPT_TEMPLATE" ]]; then
  echo "ERROR: missing AppleScript template: $APPLESCPT_TEMPLATE" >&2
  exit 1
fi

if [[ -n "$EULA_FILE" && ! -f "$EULA_FILE" ]]; then
  echo "ERROR: EULA file not found: $EULA_FILE" >&2
  exit 1
fi

command -v xcrun >/dev/null 2>&1 || { echo "ERROR: xcrun not found (install Xcode Command Line Tools)." >&2; exit 1; }
xcrun notarytool --help >/dev/null 2>&1 || { echo "ERROR: notarytool not available (need Xcode 13+)." >&2; exit 1; }

# ----------------------------
# Paths and temp
# ----------------------------
APP_NAME="$(basename "$APP_PATH")"          # NetDia.app
APP_BASENAME="${APP_NAME%.app}"            # NetDia

WORK_ROOT="$(mktemp -d -t netdia-dmg.XXXXXX)"
STAGE_DIR="$WORK_ROOT/stage"
ZIP_PATH="$WORK_ROOT/${APP_BASENAME}-notary.zip"

RW_DMG="$WORK_ROOT/rw.dmg"
MOUNT_DIR=""
DEV_NAME=""

cleanup() {
  set +e
  if [[ -n "${DEV_NAME}" ]]; then
    hdiutil detach "${DEV_NAME}" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_ROOT" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ----------------------------
# 1. Sign .app
# ----------------------------
echo "==> Signing app: $APP_PATH"
codesign --force --deep --timestamp --options runtime --sign "$APPLE_SIGN_ID" "$APP_PATH"

echo "==> Verifying app signature"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

# ----------------------------
# 2. Notarize .app (zip)
# ----------------------------
echo "==> Zipping app for notarization: $ZIP_PATH"
rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"

echo "==> Submitting app to notarytool (wait)"
xcrun notarytool submit "$ZIP_PATH" \
  --key "$APPLE_NOTARY_KEY_PATH" \
  --key-id "$APPLE_NOTARY_KEY_ID" \
  --issuer "$APPLE_NOTARY_ISSUER_ID" \
  --wait

# ----------------------------
# 3. Staple .app
# ----------------------------
echo "==> Stapling app"
xcrun stapler staple "$APP_PATH"
xcrun stapler validate "$APP_PATH"

echo "==> Gatekeeper check (.app)"
spctl -a -vv --type execute "$APP_PATH" || true

# ----------------------------
# 4. Create staged folder (clean xattrs)
# ----------------------------
echo "==> Staging app for DMG"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp -R "$APP_PATH" "$STAGE_DIR/"
xattr -cr "$STAGE_DIR/$APP_NAME" || true

# Add Applications link (drag-to-install)
ln -s /Applications "$STAGE_DIR/Applications"

# ----------------------------
# 5. Build RW dmg, mount, prettify, convert to UDZO
# ----------------------------
echo "==> Creating RW DMG"
hdiutil create -volname "$VOLNAME" -srcfolder "$STAGE_DIR" -ov -format UDRW "$RW_DMG" >/dev/null

echo "==> Mounting RW DMG"

PLIST="$(hdiutil attach -readwrite -noverify -noautoopen -nobrowse -plist "$RW_DMG")"

MOUNT_INFO="$(echo "$PLIST" | plutil -extract system-entities xml1 -o - - \
  | xpath -q -e '//dict[key="mount-point"]')"

DEV_NAME="$(echo "$MOUNT_INFO" \
  | xpath -q -e '//key[.="dev-entry"]/following-sibling::string[1]/text()')"

MOUNT_DIR="$(echo "$MOUNT_INFO" \
  | xpath -q -e '//key[.="mount-point"]/following-sibling::string[1]/text()')"

if [[ -z "$DEV_NAME" || -z "$MOUNT_DIR" || ! -d "$MOUNT_DIR" ]]; then
  echo "ERROR: failed to mount RW DMG" >&2
  echo "$PLIST" >&2
  exit 1
fi

echo "    Device: $DEV_NAME"
echo "    Mounted at: $MOUNT_DIR"

# Finder cosmetics via AppleScript template (optional)
if [[ "$SKIP_APPLESCRIPT" -eq 0 ]]; then
  echo "==> Applying Finder layout via AppleScript"
  TMP_SCPT="$(mktemp -t netdia-dmg.scpt.XXXXXX)"

  # Build AppleScript substitutions
  POSITION_CLAUSE=$(
    cat <<EOF
set position of item "$APP_NAME" to {$APP_ICON_X, $APP_ICON_Y}
set position of item "Applications" to {$APPS_LINK_X, $APPS_LINK_Y}
EOF
  )

  # No background, no hidden file repositioning by default
  BACKGROUND_CLAUSE=""
  REPOSITION_HIDDEN_FILES_CLAUSE=""
  HIDING_CLAUSE=""
  APPLICATION_CLAUSE=""
  QL_CLAUSE=""

  # Render script from template
  # NOTE: Use perl to inject multi-line placeholders safely.
  cat "$APPLESCPT_TEMPLATE" \
    | sed -e "s/WINX/$WINX/g" -e "s/WINY/$WINY/g" -e "s/WINW/$WINW/g" -e "s/WINH/$WINH/g" \
          -e "s/ICON_SIZE/$ICON_SIZE/g" -e "s/TEXT_SIZE/$TEXT_SIZE/g" \
    | perl -0777 -pe "s/POSITION_CLAUSE/$POSITION_CLAUSE/s" \
    | perl -0777 -pe "s/BACKGROUND_CLAUSE/$BACKGROUND_CLAUSE/s" \
    | perl -0777 -pe "s/REPOSITION_HIDDEN_FILES_CLAUSE/$REPOSITION_HIDDEN_FILES_CLAUSE/s" \
    | perl -0777 -pe "s/HIDING_CLAUSE/$HIDING_CLAUSE/s" \
    | perl -0777 -pe "s/APPLICATION_CLAUSE/$APPLICATION_CLAUSE/s" \
    | perl -0777 -pe "s/QL_CLAUSE/$QL_CLAUSE/s" \
    > "$TMP_SCPT"

  # Finder needs a moment sometimes...
  sleep 2
  /usr/bin/osascript "$TMP_SCPT" "$(basename "$MOUNT_DIR")" || {
    echo "WARN: AppleScript layout failed. DMG will still be usable." >&2
  }
  rm -f "$TMP_SCPT"
else
  echo "==> Skipping AppleScript layout (--skip-applescript)"
fi

# Remove noisy stuff if present
rm -rf "$MOUNT_DIR/.fseventsd" >/dev/null 2>&1 || true

echo "==> Detaching RW DMG"
hdiutil detach "$DEV_NAME" >/dev/null
DEV_NAME=""
MOUNT_DIR=""

echo "==> Converting to compressed UDZO: $OUT_DMG"
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$OUT_DMG" >/dev/null

# ----------------------------
# 6. Optional: Add EULA resources
# ----------------------------
if [[ -n "$EULA_FILE" ]]; then
  echo "==> Adding EULA resources"
  if [[ ! -f "$EULA_TEMPLATE_XML" ]]; then
    echo "ERROR: missing EULA template: $EULA_TEMPLATE_XML" >&2
    exit 1
  fi

  EULA_FORMAT="$(file -b "$EULA_FILE")"
  if [[ "$EULA_FORMAT" == Rich\ Text\ Format* ]]; then
    EULA_FMT_KEY="RTF "
  else
    EULA_FMT_KEY="TEXT"
  fi

  # Base64 encode and wrap at 52 chars like create-dmg does (udifrez expects this style)
  EULA_DATA="$(openssl base64 -in "$EULA_FILE" | tr -d '\n' | awk '{gsub(/.{52}/,"&\n")}1' \
    | sed 's/^/\t\t\t/')"

  TMP_EULA_XML="$(mktemp -t netdia-eula.XXXXXX).xml"
  # Render xml
  perl -0777 -pe "s/\\\$\\{EULA_FORMAT\\}/$EULA_FMT_KEY/g; s/\\\$\\{EULA_DATA\\}/$EULA_DATA/g" \
    "$EULA_TEMPLATE_XML" > "$TMP_EULA_XML"

  hdiutil udifrez -xml "$TMP_EULA_XML" '' -quiet "$OUT_DMG" || {
    echo "ERROR: Failed to add EULA resources" >&2
    exit 1
  }
  rm -f "$TMP_EULA_XML"
fi

# ----------------------------
# 7. Sign DMG
# ----------------------------
echo "==> Signing DMG"
codesign --force --timestamp --sign "$APPLE_SIGN_ID" "$OUT_DMG"
codesign --verify --verbose=2 "$OUT_DMG"

# ----------------------------
# 8. Notarize + staple DMG
# ----------------------------
echo "==> Submitting DMG to notarytool (wait)"
xcrun notarytool submit "$OUT_DMG" \
  --key "$APPLE_NOTARY_KEY_PATH" \
  --key-id "$APPLE_NOTARY_KEY_ID" \
  --issuer "$APPLE_NOTARY_ISSUER_ID" \
  --wait

echo "==> Stapling DMG"
xcrun stapler staple "$OUT_DMG"
xcrun stapler validate "$OUT_DMG"

echo "==> Gatekeeper check (.dmg)"
spctl -a -vv --type install "$OUT_DMG" || true

echo
echo "All Done!"
echo "DMG: $OUT_DMG"
