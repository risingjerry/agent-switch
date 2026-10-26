# Unified MCP Registry

Fusebox treats `~/.config/fusebox/config.json` as the source of truth
for MCP definitions and `~/.config/fusebox/secrets.env` as the source of
truth for credential values.

## Neutral first run

A new registry is empty. Fusebox does not preinstall search, social, or
provider-specific MCPs and does not request unrelated credentials.

```bash
fusebox write-default-config
fusebox mcp list
```

## Import and adoption

Preview MCPs found in Claude Code, Claude Desktop, Codex, and Hermes:

```bash
fusebox mcp import --dry-run --json
```

Adoption is explicit:

```bash
fusebox mcp import --adopt
```

The adopt flow validates every source first, classifies credential-shaped
environment names, stores their values without printing them, backs up the
native files, removes the imported source entries, and reconciles wrapper-backed
`agent-*` projections. Non-secret environment values stay in central config.
The native app performs this same dry-run first and shows the MCP IDs and secret
names before the user can confirm adoption.

Version 0.2 imports user-level command/stdio entries. It deliberately does not
rewrite native HTTP/SSE or OAuth definitions into an unverified transport
bridge. Those transports need a dedicated adapter that preserves each client's
authentication semantics; see the roadmap. Discovery reports those entries as
`skipped` by app and ID, and adoption leaves them untouched.

## Registry lifecycle

```bash
fusebox mcp add NAME --command COMMAND [--arg ARG] [--secret NAME] [--app APP]
fusebox mcp set NAME --command COMMAND [options]
fusebox mcp enable NAME
fusebox mcp disable NAME
fusebox mcp remove NAME
fusebox mcp list --json
```

Supported app IDs are `claude`, `claude_desktop`, `codex`, and `hermes`.
Disabling an MCP removes its managed projections and stale wrapper on the next
reconcile without deleting stored credential values.

`--env NAME=VALUE` is only for non-secret settings. Names that look like keys,
tokens, passwords, credentials, or authentication values are rejected and must
be declared with `--secret NAME`; their values are written separately through
the Secret UI or `fusebox secret set --stdin NAME`.

## Ownership

Fusebox owns only `agent-*` entries and its marked instruction blocks.
Unrelated native configuration is preserved. Existing files are written
atomically and backups are kept in the private mode-`0700` backup directory.
