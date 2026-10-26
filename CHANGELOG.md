# Changelog

All notable changes to Fusebox (formerly Agent Switch) are documented here.

## 0.4.0 - 2026-10-26

Renamed to **Fusebox** and rewritten in Rust as a single binary.

### Added

- Two states for every MCP server and Skill: resident or on demand (the default).
- `agent-hub`: one lazy MCP proxy per agent; servers start on first call with
  only their own secrets and stop when idle. Large servers are listed by name.
- `fusebox find`: one ranked catalog of Skills and MCP servers (BM25, zh/en
  synonyms, per-Skill aliases, usage prior), cached for sub-10 ms lookups.
- Claude Code `name-only` projection with a one-line description for the 30
  most used on-demand Skills; the same list for Codex and DeepSeek Harness.
- DeepSeek Harness as a managed agent.
- Usage from Claude Code, Codex and DeepSeek Harness transcripts;
  `fusebox advise` with exact commands, including a drop-off alert.
- `fusebox eval` to score discovery on your own sessions.
- `skills add-source`, `drop` / `restore`; the Skill Hub defaults to
  `~/.config/fusebox/skill-hub` and is created on first use.
- Redesigned bilingual macOS app with in-app language switch; the CLI ships
  inside the app bundle.

### Changed

- Config moves to `~/.config/fusebox` (the old `~/.config/agent-switch` is
  still read). `agent-switch` can stay as a symlink to `fusebox`.

### Removed

- The Python implementation and the CC Switch import.

## 0.3.0 - 2026-08-11

### Added

- Complete English and Simplified Chinese native-app localization catalogs.
- Real, sanitized product visuals and privacy-safe repository activity graphics.
- Project metadata for software citation and answer-engine discovery.

### Changed

- Homebrew is now the canonical stranger-first install and update path.
- README onboarding, trust boundaries, recovery guidance, and bilingual parity were refreshed.
- Test credentials now use unmistakably synthetic, non-provider-shaped fixture values.

### Security

- Repository metrics keep owner-only Traffic collection inside GitHub Actions and publish only a generated aggregate image.
- Release guidance continues to disclose ad-hoc signing and the absence of Apple notarization.

## 0.2.0 - 2026-07-23

### Added

- Neutral empty registry for new installations.
- Unified MCP `list`, `add`, `set`, `enable`, `disable`, `remove`, and native
  `import`/`--adopt` commands.
- macOS MCP editor, target selection, import/adoption, enable/disable, and
  removal actions.
- Native import preview showing MCP IDs and secret names before adoption.
- Credential-to-MCP consumer mapping in status and the Secret UI.
- Detected-agent-only reconciliation for clean first-run behavior.
- Reproducible universal macOS release packaging for GitHub Releases.
- Homebrew Formula and Cask distribution through the project-owned Tap.

### Security

- Generated wrappers no longer source or export the complete secret store.
- Each MCP receives only its declared credentials; inherited sensitive
  variables and unrelated stored credentials are removed.
- Native-config adoption backs up files privately before removing inline MCPs.
- Backup names include both target-path and content digests to prevent collisions.
- Credential-shaped stdio arguments are migrated to declared secret placeholders.
- Positional secret values are rejected.
- Doctor warns about unpinned `npx` packages.

### Changed

- The macOS app prefers an installed Agent Switch CLI and no longer requires a
  source checkout when that CLI is available.
- `doctor --strict` now fails for drift or missing required credentials as well
  as blocked targets.
- The public alpha is ad-hoc signed and explicitly documented as not notarized.

## 0.1.3 - 2026-07-13

- Initial public repository preparation with CLI, native dashboard, secret
  operations, MCP projections, Skill inventory, and CC Switch compatibility.
