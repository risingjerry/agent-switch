//! Managed runtime policy written into each agent's global instructions.

use std::path::{Path, PathBuf};

use crate::paths::Paths;
use crate::util::{Result, WriteResult, read_text, resolve_lenient, write_if_changed};

pub const MANAGED_START: &str = "<!-- BEGIN FUSEBOX MANAGED INSTRUCTIONS -->";
pub const MANAGED_END: &str = "<!-- END FUSEBOX MANAGED INSTRUCTIONS -->";
/// Markers written before the rename; replaced in place on the next sync.
const LEGACY_START: &str = "<!-- BEGIN AGENT SWITCH MANAGED INSTRUCTIONS -->";
const LEGACY_END: &str = "<!-- END AGENT SWITCH MANAGED INSTRUCTIONS -->";

fn find_block(text: &str) -> Option<(usize, usize)> {
    [(MANAGED_START, MANAGED_END), (LEGACY_START, LEGACY_END)].iter().find_map(|(s, e)| {
        let start = text.find(s)?;
        let end = text.find(e)?;
        (end > start).then_some((start, end + e.len()))
    })
}

pub fn skill_hub_dir(paths: &Paths) -> PathBuf {
    let hub = match std::env::var("SKILL_HUB_HOME") {
        Ok(v) if !v.is_empty() => crate::util::expand_user(&v),
        _ => paths.user_home.join("AgentWorkspace").join("skill-hub"),
    };
    resolve_lenient(&hub)
}

fn core_policy(paths: &Paths) -> String {
    let hub = skill_hub_dir(paths);
    let global_skills = resolve_lenient(&paths.user_home.join(".agents").join("skills"));
    let secrets = paths.secrets_file.display();
    let config = paths.config_file.display();
    let hub = hub.display();
    let global_skills = global_skills.display();
    format!(
        "# Fusebox Runtime Policy

Fusebox is the single source of truth on this machine for agent secrets, MCP servers, CLIs and Skills.

Secrets and MCP:
- Secrets live only in `{secrets}`; never in project `.env`, README, native app configs, chats, shell history, or logs. Never print a secret value unless the user explicitly asks for a local-only diagnostic and redaction is impossible.
- Write with `fusebox secret set --stdin NAME` (or `--fd N`), never as a command argument; read only via `fusebox secret get --fd N NAME` into a non-TTY descriptor; list names with `fusebox secret list`.
- `{config}` is the central tool config. Run `fusebox doctor` before MCP/tool changes and `fusebox reconcile` to regenerate native configs. Generated MCP entries stay in the `agent-*` namespace. A new provider gets only its secret name in config.
- MCPs in hub mode start on first use and stop when idle: `fusebox mcp hub ID` / `fusebox mcp direct ID`.

Skills (sources in `{hub}`):
- Inspect with `fusebox skills status`; `fusebox skills usage` shows what each agent actually used.
- A downloaded Skill is dormant until enabled for a project profile or the `global` profile: `fusebox skills enable PROFILE SKILL --source SOURCE --path PATH` / `fusebox skills disable PROFILE SKILL`, then `fusebox skills sync PROFILE --prune`.
- Only on an explicit user request run `fusebox skills sync-global --prune` or `fusebox skills prune --apply`. If unexpected global Skills appear, report them without changing activation and do not block unrelated tasks.
- Third-party Skills live once under `vendor/` with a locked commit; self-authored ones in Git-managed `own/` or their own repos. `{global_skills}` is generated output: never copy Skills there by hand. Do not run broad global installs such as `skills add ... -g --all` unless asked.
- Never store credentials in Skill files, Skill references, Skill scripts, registry files, lock files, or audit reports.
"
    )
}

pub fn codex(paths: &Paths) -> String {
    core_policy(paths)
        + "
Codex-specific rule:
- This file is loaded from `model_instructions_file`; every new Codex session should follow it.
"
}

pub fn claude(paths: &Paths) -> String {
    core_policy(paths)
        + "
Claude-specific rule:
- Keep global Claude Code behavior aligned with Fusebox. If project-local `CLAUDE.md` files conflict on secret storage, the Fusebox secret path wins.
"
}

pub fn hermes(paths: &Paths) -> String {
    core_policy(paths)
        + "
Hermes-specific rule:
- Hermes provider switching may be handled elsewhere, but MCP credentials and tool secrets remain managed by Fusebox.
"
}

pub fn merge_managed_block(current: &str, body: &str) -> String {
    let block = format!("{MANAGED_START}\n{}\n{MANAGED_END}\n", body.trim_end());
    if let Some((start, end)) = find_block(current) {
        let prefix = current[..start].trim_end();
        let mut merged = String::new();
        if !prefix.is_empty() {
            merged.push_str(prefix);
            merged.push_str("\n\n");
        }
        merged.push_str(&block);
        merged.push_str(current[end..].trim_start_matches('\n'));
        return format!("{}\n", merged.trim_end());
    }
    let base = current.trim_end();
    if base.is_empty() { block } else { format!("{base}\n\n{block}") }
}

pub fn contains_managed_block(path: &Path) -> bool {
    read_text(path).map(|t| find_block(&t).is_some()).unwrap_or(false)
}

/// (target name, path, desired content, mode)
pub type Target = (&'static str, PathBuf, String, Option<u32>);

pub fn targets(paths: &Paths, apps: &[&str]) -> Result<Vec<Target>> {
    let has = |a: &str| apps.contains(&a);
    let mut out = Vec::new();
    if has("codex") {
        out.push(("instructions.codex", paths.codex_instructions.clone(), codex(paths), Some(0o600)));
    }
    if has("claude") {
        out.push(("instructions.claude", paths.claude_instructions.clone(), claude(paths), Some(0o600)));
    }
    if has("hermes") {
        out.push(("instructions.hermes", paths.hermes_instructions.clone(), hermes(paths), Some(0o600)));
    }
    if has("claude") {
        let current = read_text(&paths.claude_global_instructions)?;
        out.push((
            "instructions.claude_global",
            paths.claude_global_instructions.clone(),
            merge_managed_block(&current, &claude(paths)),
            None,
        ));
    }
    if has("hermes") {
        let current = read_text(&paths.hermes_soul)?;
        out.push((
            "instructions.hermes_soul",
            paths.hermes_soul.clone(),
            merge_managed_block(&current, &hermes(paths)),
            Some(0o600),
        ));
    }
    Ok(out)
}

pub fn write(paths: &Paths, apps: &[&str]) -> Result<Vec<WriteResult>> {
    targets(paths, apps)?
        .into_iter()
        .map(|(_, path, text, mode)| write_if_changed(&path, &text, mode, Some(&paths.backup_dir)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_block_in_place() {
        let body = "policy";
        let fresh = merge_managed_block("", body);
        assert_eq!(fresh, format!("{MANAGED_START}\npolicy\n{MANAGED_END}\n"));
        let existing = format!("# Mine\n\n{MANAGED_START}\nold\n{MANAGED_END}\n\n# After\n");
        assert_eq!(
            merge_managed_block(&existing, body),
            format!("# Mine\n\n{MANAGED_START}\npolicy\n{MANAGED_END}\n# After\n")
        );
        assert_eq!(
            merge_managed_block("# Mine\n", body),
            format!("# Mine\n\n{MANAGED_START}\npolicy\n{MANAGED_END}\n")
        );
        let legacy = format!("{LEGACY_START}\nold\n{LEGACY_END}\n# After\n");
        assert_eq!(merge_managed_block(&legacy, body), format!("{MANAGED_START}\npolicy\n{MANAGED_END}\n# After\n"));
    }
}
