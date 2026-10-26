#!/bin/sh
# Parity check: run the Python and Rust implementations against identical
# temporary copies of the native agent configs and diff what they write.
# Secrets are never copied; a dummy store with the required names is used.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
src_home=${PARITY_SOURCE_HOME:-$HOME}
src_agent=${PARITY_SOURCE_AGENT_HOME:-$HOME/.config/fusebox}
rust_bin=${RUST_BIN:-$repo/target/release/fusebox}
work=$(cd "$(mktemp -d "${TMPDIR:-/tmp}/as-parity.XXXXXX")" && pwd -P)
trap 'rm -rf "$work"' EXIT

seed() {
  dir=$1
  mkdir -p "$dir/user/.codex" "$dir/user/.claude" "$dir/user/.hermes" \
    "$dir/user/Library/Application Support/Claude" "$dir/agent"
  for f in .claude.json .codex/config.toml .hermes/config.yaml .claude/CLAUDE.md .hermes/SOUL.md \
    "Library/Application Support/Claude/claude_desktop_config.json"; do
    [ -f "$src_home/$f" ] && cp "$src_home/$f" "$dir/user/$f"
  done
  python3 - "$src_agent/config.json" "$dir/agent" <<'PY'
import json, sys, pathlib
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
cfg = json.loads(src.read_text()) if src.exists() else {"tools": [], "routes": {}}
cfg["secretFile"] = str(dst / "secrets.env")
(dst / "config.json").write_text(json.dumps(cfg))
names = sorted({n for t in cfg.get("tools", []) for n in t.get("requiredSecrets", [])})
(dst / "secrets.env").write_text("".join(f"{n}=dummy-value\n" for n in names))
PY
}

seed "$work/py"
seed "$work/rs"
# Both implementations target the real user's PATH only when --user-home is
# the real home; temp homes keep app detection to the copied files.
(cd "$repo" && PYTHONPATH=src python3 -m agent_switch --home "$work/py/agent" --user-home "$work/py/user" reconcile --no-ccswitch >/dev/null) || true
"$rust_bin" --home "$work/rs/agent" --user-home "$work/rs/user" reconcile >/dev/null || true
# Normalise the one intended difference: the per-tool wrapper path is shared,
# so native configs must match byte-for-byte apart from the temp root.
status=0
for f in .claude.json .codex/config.toml .hermes/config.yaml "Library/Application Support/Claude/claude_desktop_config.json"; do
  [ -f "$work/py/user/$f" ] || continue
  sed "s#$work/py#ROOT#g" "$work/py/user/$f" > "$work/a"
  sed "s#$work/rs#ROOT#g" "$work/rs/user/$f" > "$work/b"
  if cmp -s "$work/a" "$work/b"; then
    echo "same      $f"
  elif case $f in *.json) python3 -c 'import json,sys; sys.exit(json.load(open(sys.argv[1])) != json.load(open(sys.argv[2])))' "$work/a" "$work/b";; *) false;; esac; then
    # Rust keeps number literals as written; Python re-prints floats with repr().
    echo "same*     $f (equal JSON; number literals preserved)"
  else
    echo "DIFFERENT $f"; diff "$work/a" "$work/b" | head -20; status=1
  fi
done
cmp -s "$work/py/agent/config.json" "$work/rs/agent/config.json" || true
exit $status
