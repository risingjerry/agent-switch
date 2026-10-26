# Roadmap

Fusebox 0.4 is the Rust rewrite: one binary, two states (resident or on
demand) for every MCP server and Skill, a lazy MCP hub, one catalog with
`fusebox find`, usage-based `advise`, and `eval` to measure discovery.

Next, roughly in order:

1. Developer ID signing and notarization for the app, then drop the cask's
   quarantine removal.
2. Remote (Streamable HTTP / SSE) MCP servers behind the hub, with OAuth left
   to the server.
3. Codex: use its own on-demand Skill switches once they can list names without
   descriptions.
4. Per-project usage priors in agents' name lists, not only in `find`.
5. A first-run wizard in the app that previews every file it will touch.

Ideas and bug reports: open an issue.
