#!/bin/zsh
# Build the release artifacts into dist/ (or $1):
#   fusebox-VERSION-macos-universal.tar.gz   the CLI (arm64 + x86_64)
#   Fusebox-VERSION-macos-universal.zip      the app, with the CLI inside
set -euo pipefail

ROOT="${0:A:h:h}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
PROJECT="$ROOT/macos-app/AgentSwitch/AgentSwitch.xcodeproj"
OUTPUT_DIR="${1:-$ROOT/dist}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fusebox-release.XXXXXX")"
APP="$WORK/Build/Products/Release/Fusebox.app"
CLI_ARCHIVE="$OUTPUT_DIR/fusebox-$VERSION-macos-universal.tar.gz"
APP_ARCHIVE="$OUTPUT_DIR/Fusebox-$VERSION-macos-universal.zip"
trap 'rm -rf "$WORK"' EXIT

fail() { print -u2 "Fusebox release build failed: $*"; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || fail "macOS is required."
for command in cargo rustup xcodebuild codesign ditto lipo shasum; do
  command -v "$command" >/dev/null 2>&1 || fail "$command is required."
done
mkdir -p "$OUTPUT_DIR"

# CLI: one universal binary.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --locked --target "$target" --manifest-path "$ROOT/Cargo.toml"
done
lipo -create -output "$WORK/fusebox" \
  "$ROOT/target/aarch64-apple-darwin/release/fusebox" \
  "$ROOT/target/x86_64-apple-darwin/release/fusebox"
codesign --force --sign - "$WORK/fusebox"
[[ "$("$WORK/fusebox" --version)" == *"$VERSION"* ]] || fail "CLI version does not match $VERSION."
tar -C "$WORK" -czf "$CLI_ARCHIVE" fusebox

# App, with the CLI in Resources so the app works without a separate install.
xcodebuild -project "$PROJECT" -scheme AgentSwitch -configuration Release \
  -destination "generic/platform=macOS" -derivedDataPath "$WORK" -quiet \
  ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO build
[[ -d "$APP" ]] || fail "Xcode did not produce $APP"
APP_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
[[ "$APP_VERSION" == "$VERSION" ]] || fail "App version $APP_VERSION does not match $VERSION."
cp "$WORK/fusebox" "$APP/Contents/Resources/fusebox"

# Ad-hoc signed, not notarized (no Apple Developer ID yet); the Homebrew cask
# clears the quarantine flag and says so.
xattr -cr "$APP"
codesign --force --deep --sign - --options runtime --timestamp=none "$APP"
codesign --verify --deep --strict "$APP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$APP_ARCHIVE"

cd "$OUTPUT_DIR"
for f in "${CLI_ARCHIVE:t}" "${APP_ARCHIVE:t}"; do shasum -a 256 "$f" > "$f.sha256"; done
print "Built $CLI_ARCHIVE"
print "Built $APP_ARCHIVE"
