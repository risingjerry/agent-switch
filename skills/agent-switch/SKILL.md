---
name: agent-switch
description: Manage local AI-agent MCP servers and credentials on macOS with the `agent-switch` CLI. Use when the user wants to add, import, enable, disable or remove an MCP server for Codex / Claude Code / Claude Desktop / Hermes, store or check an API key for an MCP, fix MCP config drift between agents, see which agent CLIs are installed, or says "register this MCP once for all my agents", "where do I put this API key", "agent-switch doctor", "同步 MCP 配置", "统一管理密钥".
---

# Agent Switch

`agent-switch` keeps one central registry of user-level stdio MCP servers and one private secret store, then projects them into each agent's native config as `agent-*` entries. macOS only.

## First contact

```bash
agent-switch --version
agent-switch doctor --json       # health, drift, missing secret NAMES
```

If missing:

```bash
brew tap risingjerry/tap && brew trust --tap risingjerry/tap && brew install --cask risingjerry/tap/agent-switch
```

## Mental model

- Central config: `~/.config/agent-switch/config.json`. Secret values: `~/.config/agent-switch/secrets.env` (mode 0600). Backups: `~/.config/agent-switch/backups/`.
- `doctor` and `reconcile --dry-run` are read-only. `reconcile` writes native configs atomically with backups.
- Each MCP wrapper injects only the secret names that MCP declares.

## Safety rules

- Never print a secret value, never put one in an argument, chat, `.env`, or log.
- Write secrets from stdin or an inherited descriptor: `some-command | agent-switch secret set --stdin NAME`, or `agent-switch secret set --fd N NAME`. If the user must type the value, tell them to use the Agent Switch app or run the command themselves; do not ask for it in chat.
- Read only with `agent-switch secret get --fd N NAME` into a non-TTY descriptor, and only if the user explicitly asks. For checking existence use `agent-switch secret list` (names only).
- Always `doctor` before changing MCPs; always preview (`--dry-run`) before `mcp import --adopt` and `reconcile`. Ask before adopting or deleting.
- Do not edit `agent-*` entries in native configs by hand.

## Recipes

```bash
# Inventory
agent-switch agents --json            # detected agents, policy enrollment
agent-switch clis --json              # installed AI CLIs
agent-switch mcp list --json          # central registry
agent-switch secret list --json       # names only
agent-switch skills list --json       # optional Skill Hub inventory

# Adopt existing MCPs from native configs
agent-switch mcp import --dry-run --json
agent-switch mcp import --adopt       # after the user approves the preview

# Add an MCP (id gets the agent- prefix; targets: claude, claude_desktop, codex, hermes)
agent-switch mcp add filesystem --command npx \
  --arg=-y --arg=@modelcontextprotocol/server-filesystem@1.0.0 \
  --app codex --app claude
agent-switch mcp add search --command npx --arg=-y --arg=SOME_PKG@1.0.0 --secret SEARCH_API_KEY

# Edit, toggle, remove
agent-switch mcp set NAME --command ...    # same flags as add
agent-switch mcp disable NAME
agent-switch mcp enable NAME
agent-switch mcp remove NAME

# Apply and verify
agent-switch reconcile --dry-run
agent-switch reconcile
agent-switch doctor --strict          # non-clean managed state is an error
```

`--secret NAME` grants a secret name to the MCP; the value is stored separately via `secret set`. Use `--env NAME=VALUE` only for non-secret settings.

## Errors

- `doctor` reports missing secret names: wrappers fail closed until the user adds them via `secret set --stdin`.
- Blocked target or invalid config: fix the native file syntax (backups are in `~/.config/agent-switch/backups/`), re-run `doctor`, then `reconcile`.
- CC Switch schema drift: Agent Switch blocks database writes; do not bypass.
- Unpinned `npx` package warning: pin a version in `--arg`.

## More

Docs in https://github.com/risingjerry/agent-switch: `docs/mcp-registry.md`, `docs/secrets-and-wrappers.md`, `docs/recovery.md`.
