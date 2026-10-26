#!/bin/zsh
# Build and install Fusebox from this checkout: the CLI into ~/.local/bin and
# (on macOS with Xcode) the app into ~/Applications. Existing files are moved
# to a timestamped backup first.
set -euo pipefail

ROOT="${0:A:h:h}"
BIN_DIR="${FUSEBOX_BIN_DIR:-$HOME/.local/bin}"
fail() { print -u2 "Fusebox install failed: $*"; exit 1; }

command -v cargo >/dev/null 2>&1 || fail "Rust is required: https://rustup.rs"
cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$BIN_DIR"
if [[ -e "$BIN_DIR/fusebox" && ! -L "$BIN_DIR/fusebox" ]]; then
  mv "$BIN_DIR/fusebox" "$BIN_DIR/fusebox.backup-$(date +%Y%m%d%H%M%S)"
fi
# Replace rather than overwrite in place: macOS kills a re-signed binary
# copied over a running one.
rm -f "$BIN_DIR/fusebox"
cp "$ROOT/target/release/fusebox" "$BIN_DIR/fusebox"
"$BIN_DIR/fusebox" --version
[[ ":$PATH:" == *":$BIN_DIR:"* ]] || print "Add $BIN_DIR to PATH to run fusebox."

if [[ "$(uname -s)" == "Darwin" ]] && xcodebuild -version >/dev/null 2>&1; then
  "$ROOT/macos-app/AgentSwitch/install.sh"
fi
print "Next: fusebox doctor"
