---
name: fusebox
description: Manage this machine's agent setup through the local `fusebox` CLI — every MCP server and Skill is either resident or on demand, globally or per project; plus secrets (names only) and sync to Claude Code, Codex, DeepSeek Harness and Hermes. Use when the user asks what to optimise or clean up, which Skills or MCP servers should be resident or on demand, why a tool or skill is missing, how much each is used, or to add/rotate an API key. Also use to find an on-demand skill for the current task (`fusebox skills search`). Not for editing project code.
---

# Fusebox

Fusebox is the single place this machine keeps agent secrets, MCP servers and Skills. The user watches it in the Fusebox app; **you** make the changes, through the CLI, after the user agrees.

## Find what you need (do this before saying a capability is missing)

`fusebox find WORDS` ranks every Skill and MCP server for a task (English or Chinese; describe the capability, not the user's exact words). Each hit says how to load it.

## Two states, nothing is ever "off"

Everything is **on demand** unless the user explicitly asked for it to be resident.

- **Resident**: loaded into every session (globally, or only in one project). For a Skill: in your skill list. For an MCP: started with the session.
- **On demand**: costs nothing until needed, but always reachable.
  - Skill → `fusebox skills search WORDS`, then `fusebox skills show NAME` and follow it.
  - MCP → the `agent-hub` MCP: `fusebox_servers` lists them (pass `server` to start one and see its tools), `fusebox_call` calls a tool.

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

- MCP: `fusebox mcp direct ID` (resident) or `fusebox mcp hub ID` (on demand; add `--no-warm` for servers that open a login page on start). Then `fusebox reconcile`.
- Turning on demand renames tools to `mcp__agent-hub__<short-id>__<tool>`; `advise` names the Skills that hard-code the old names — update those Skills too.
- Which agents get an MCP: `fusebox mcp set ID --command ... --app claude --app codex --app deepseek ...` (repeat `--app`; omitting all means every agent).
- Skills (sync happens automatically): resident with `fusebox skills enable PROFILE NAME...`, on demand with `fusebox skills disable PROFILE NAME...`, resident only in one project with `fusebox skills move NAME --to PROJECT`. PROFILE is `global` or a project profile (`fusebox skills status` lists them). Files are never deleted.
- Profiles may pin skills in their `keep` list; `advise` never suggests moving those.
- Always finish MCP changes with `fusebox reconcile`.

## Secrets — hard rules

- Never print, echo, log or paste a secret value. Never pass one as a command argument.
- To store one, the user supplies the value: `fusebox secret set --stdin NAME` with the value on stdin, or `--fd N`.
- To use one in a script: `fusebox secret get --fd 3 NAME 3>&1 1>/dev/null` into a variable, never to the terminal.
- Missing secrets in `advise` are `needsHuman`: ask the user for the value; do not invent or fetch one.

## Do not

- Edit `~/.claude.json`, `~/.codex/config.toml`, `~/.dsh/cordis.patch.yml`, `~/.hermes/config.yaml` or generated `agent-*` entries by hand — change Fusebox and run `fusebox reconcile`.
- Copy Skills into `~/.agents/skills` by hand; that directory is generated.
