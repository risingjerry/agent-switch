#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
DERIVED_DATA="${TMPDIR:-/tmp}/FuseboxInstallDerivedData"
PRODUCT="$DERIVED_DATA/Build/Products/Release/Fusebox.app"
INSTALL_DIR="${FUSEBOX_APP_DIR:-$HOME/Applications}"
DESTINATION="$INSTALL_DIR/Fusebox.app"
trap 'rm -rf "$DERIVED_DATA"' EXIT

cd "$SCRIPT_DIR"
rm -rf "$DERIVED_DATA"
xcodebuild \
  -project AgentSwitch.xcodeproj \
  -scheme AgentSwitch \
  -configuration Release \
  -derivedDataPath "$DERIVED_DATA" \
  CODE_SIGNING_ALLOWED=NO \
  build

test -d "$PRODUCT"
mkdir -p "$INSTALL_DIR"
pkill -x "Fusebox" 2>/dev/null || true
rm -rf "$DESTINATION"
ditto "$PRODUCT" "$DESTINATION"
codesign --force --deep --sign - --options runtime "$DESTINATION"

/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -f -R -trusted "$DESTINATION"

open "$DESTINATION"
echo "Installed $DESTINATION"
