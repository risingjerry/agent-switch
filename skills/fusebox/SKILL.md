---
name: fusebox
description: Manage this machine's agent setup through the local `fusebox` CLI — every MCP server and Skill is either resident or on demand, globally or per project; plus secrets (names only) and sync to Claude Code, Codex, DeepSeek Harness and Hermes. Use when the user asks what to optimise or clean up, which Skills or MCP servers should be resident or on demand, why a tool or skill is missing, how much each is used, to add/rotate an API key, or to install a Skill, MCP server or CLI from a link, repo or package (even when Fusebox is not named). Also use to find an on-demand skill for the current task (`fusebox skills search`). Not for editing project code.
---

# Fusebox

Fusebox is the single place this machine keeps agent secrets, MCP servers and Skills. The user watches it in the Fusebox app; **you** make the changes, through the CLI, after the user agrees.

## Find what you need (do this before saying a capability is missing)

`fusebox find WORDS` ranks every Skill and MCP server for a task (English or Chinese; describe the capability, not the user's exact words). Each hit says how to load it.

## Two states, nothing is ever "off"

Everything is **on demand** unless the user explicitly asked for it to be resident.

- **Resident**: loaded into every session (globally, or only in one project). For a Skill: in your skill list. For an MCP: started with the session.
- **On demand**: costs nothing until needed, but always reachable. Agents see every on-demand name; the 30 most used also show a one-line description.
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

## Installing something new

When the user asks to install / add / set up a Skill, MCP server or CLI — a GitHub link, `owner/repo`, npm or PyPI package, or a README snippet — do it through Fusebox, never through `claude mcp add`, `codex mcp add`, `npx skills add` or copying files into an agent's folders.

1. Read what it ships (SKILL.md and scripts, or the server's command and the keys it needs). Tell the user about anything unsafe before installing.
2. Skill: `fusebox install skill SOURCE` — SOURCE is a GitHub URL (`/tree/REF/PATH` works), `owner/repo` or a local folder. Third-party repos are cloned into `vendor/` at a locked commit. New Skills are on demand; `--resident-in PROFILE` only when the user asks.
3. MCP server: `fusebox install mcp ID --command CMD --arg A ... [--secret NAME]...`, or `--url URL` for a remote (HTTP) server. On demand by default; `--resident` only when asked. If a key is missing, ask the user for it and store it with `fusebox secret set --stdin NAME`.
4. CLI: install with its package manager (brew, npm, uv, pipx). Keys go into Fusebox secrets, never `.env` or shell profiles. A Skill or MCP it ships goes through steps 2–3.
5. Report what was installed (`--json` lists the Skills found or the MCP's tool count). Installs sync every agent; new sessions see them.

## Making changes

- MCP: `fusebox mcp direct ID` (resident) or `fusebox mcp hub ID` (on demand; add `--no-warm` for servers that open a login page on start).
- Turning on demand renames tools to `mcp__agent-hub__<short-id>__<tool>`; `advise` names the Skills that hard-code the old names — update those Skills too.
- Which agents get an MCP: `fusebox mcp set ID --command ... --app claude --app codex --app deepseek ...` (repeat `--app`; omitting all means every agent).
- Skills (sync happens automatically): resident with `fusebox skills enable PROFILE NAME...`, on demand with `fusebox skills disable PROFILE NAME...`, resident only in one project with `fusebox skills move NAME --to PROJECT`. PROFILE is `global` or a project profile (`fusebox skills status` lists them). Files are never deleted.
- Profiles may pin skills in their `keep` list; `advise` never suggests moving those.
- Take a Skill out of the catalog entirely (files kept): `fusebox skills drop NAME...`; undo with `fusebox skills restore NAME...`. Only when the user asks.
- A Skill agents keep missing: add the words people use for it to the hub's `config/aliases.json` (`{"name": "phrase / 说法 / ..."}`); `find` and `skills search` match them.
- After changing aliases, descriptions or the catalog, run `fusebox eval`: it scores `find` on real prompt→Skill pairs from past sessions and shows the change since the last run.
- Every command above syncs all agents when it finishes (`--no-sync` skips). Run `fusebox reconcile` only after editing Fusebox files by hand.

## Secrets — hard rules

- Never print, echo, log or paste a secret value. Never pass one as a command argument.
- To store one, the user supplies the value: `fusebox secret set --stdin NAME` with the value on stdin, or `--fd N`.
- To use one in a script: `fusebox secret get --fd 3 NAME 3>&1 1>/dev/null` into a variable, never to the terminal.
- Missing secrets in `advise` are `needsHuman`: ask the user for the value; do not invent or fetch one.

## Do not

- Edit `~/.claude.json`, `~/.codex/config.toml`, `~/.dsh/cordis.patch.yml`, `~/.hermes/config.yaml` or generated `agent-*` entries by hand — change it through Fusebox.
- Copy Skills into `~/.agents/skills` by hand; that directory is generated.
