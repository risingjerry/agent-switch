---
name: fusebox
description: Manage this machine's agent setup through the local `fusebox` CLI — MCP servers (always on / on demand / off), Skills (enable, disable, clean up unused), secrets (names only) and sync to Claude Code, Codex, DeepSeek Harness and Hermes. Use when the user asks what to clean up or optimise, which Skills or MCP servers to turn off or switch to on demand, why a tool or skill is missing, how much each skill is used, or to add/rotate an API key. Not for editing project code.
---

# Fusebox

Fusebox is the single place this machine keeps agent secrets, MCP servers and Skills. The user watches it in the Fusebox app; **you** make the changes, through the CLI, after the user agrees.

## Read first

| Question | Command |
|---|---|
| What should change? (start here) | `fusebox advise --json` |
| Is everything synced? | `fusebox status --json` |
| Which MCP servers exist, and in what mode? | `fusebox mcp list --json` |
| How often is each Skill / MCP used? | `fusebox skills usage --json` (Claude Code, Codex, DeepSeek Harness sessions) |
| Which Skills are enabled where? | `fusebox skills --json` |
| Which secrets exist? (names only) | `fusebox secret list --json` |

## Workflow when the user asks "any suggestions?" / "clean this up"

1. Run `fusebox advise --json`. Each suggestion has `title`, `reason`, `commands` and `needsHuman`.
2. Summarise the suggestions for the user in plain words, grouped (sync / MCP / Skills / secrets). Mention what is kept and why.
3. Apply **only** what the user approves, by running that suggestion's `commands` in order. Never apply everything silently.
4. Finish with `fusebox status` and report what changed. New agent sessions pick up the changes; running sessions keep their old tool list.

## Making changes

- MCP mode: `fusebox mcp direct ID` (always on), `fusebox mcp hub ID` (on demand: starts on first use, stops when idle), `fusebox mcp disable ID` (off), `fusebox mcp enable ID`. Then `fusebox reconcile`.
- Turning on demand renames tools to `mcp__agent-hub__<short-id>__<tool>`; `advise` names the Skills that hard-code the old names — update those Skills too.
- Which agents get an MCP: `fusebox mcp set ID --command ... --app claude --app codex --app deepseek ...` (repeat `--app`; omitting all means every agent).
- Skills: `fusebox skills disable PROFILE NAME` / `fusebox skills enable PROFILE NAME --source SOURCE --path PATH`, then `fusebox skills sync PROFILE --prune` (`global` profile → `fusebox skills sync-global --prune`). Bulk: `fusebox skills prune --profile PROFILE --apply` (dry run without `--apply`). Disabling never deletes files.
- Always finish MCP changes with `fusebox reconcile`.

## Secrets — hard rules

- Never print, echo, log or paste a secret value. Never pass one as a command argument.
- To store one, the user supplies the value: `fusebox secret set --stdin NAME` with the value on stdin, or `--fd N`.
- To use one in a script: `fusebox secret get --fd 3 NAME 3>&1 1>/dev/null` into a variable, never to the terminal.
- Missing secrets in `advise` are `needsHuman`: ask the user for the value; do not invent or fetch one.

## Do not

- Edit `~/.claude.json`, `~/.codex/config.toml`, `~/.dsh/cordis.patch.yml`, `~/.hermes/config.yaml` or generated `agent-*` entries by hand — change Fusebox and run `fusebox reconcile`.
- Copy Skills into `~/.agents/skills` by hand; that directory is generated.
