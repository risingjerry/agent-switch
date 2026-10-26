# Fusebox — guide for coding agents

Fusebox is a Rust CLI (`fusebox`) plus a SwiftUI macOS app. It keeps agent
secrets, MCP servers and Skills in one place and writes each agent's native
config (Claude Code, Codex, DeepSeek Harness, Claude Desktop, Hermes).

## Build and test

```bash
cargo build --release            # binary: target/release/fusebox
cargo test --release --locked    # unit tests + hub end-to-end test (needs python3)
cargo clippy --release --all-targets --locked -- -D warnings
cargo fmt --check
swift build --package-path macos-app/AgentSwitch   # the app, macOS only
python3 scripts/privacy_scan.py
```

Install from this checkout: `scripts/install.sh` (CLI to `~/.local/bin`, app to
`~/Applications` when Xcode is present). Try it without touching your real
setup by pointing `HOME` at a scratch directory:
`docs/demo/make-demo-home.sh /tmp/fusebox-demo && HOME=/tmp/fusebox-demo target/release/fusebox find "merge pdfs"`.

## Layout

| Path | What |
|---|---|
| `src/main.rs` | CLI parsing and commands |
| `src/doctor.rs` | plan/apply: what each agent's config should be |
| `src/hub.rs` | `agent-hub`: lazy stdio MCP proxy (start on first call, stop when idle) |
| `src/find.rs` | one ranked catalog of Skills and MCP servers (BM25 + aliases + usage prior) |
| `src/eval.rs` | scores `find` on prompt→Skill pairs mined from Claude Code sessions |
| `src/skills.rs` | Skill Hub: sources, profiles, sync, catalog |
| `src/usage.rs` | Skill/MCP use from Claude Code, Codex and DeepSeek Harness transcripts |
| `src/claude.rs`, `src/render.rs`, `src/instructions.rs` | per-agent projections |
| `src/secrets.rs`, `src/run.rs` | secret store and per-MCP wrappers |
| `macos-app/AgentSwitch` | the app; it only calls the CLI with `--json` |

## Using the CLI as an agent

Every read command has `--json`. Start with `fusebox advise --json` and
`fusebox find WORDS --json`. Writes (`reconcile`, `mcp …`, `skills enable|disable|drop`)
change the user's agent setup: run them only when the user asked for that
change. `reconcile --dry-run` previews. The bundled skill
[`skills/fusebox/SKILL.md`](skills/fusebox/SKILL.md) is the full usage guide.

## Never

- Print, log or pass secret values as arguments. Values go in through
  `fusebox secret set --stdin NAME` and out through `secret get --fd N`.
- Edit generated native configs (`~/.claude.json`, `~/.codex/config.toml`, …)
  by hand; change Fusebox and reconcile.
- Touch MCP entries outside the `agent-*` namespace or text outside Fusebox's
  marked instruction blocks.
- Commit real Skill names, paths or transcripts in tests or docs; use
  synthetic data.
