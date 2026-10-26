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
- Every MCP server is either resident or on demand. On-demand servers run behind the `agent-hub` MCP: use its `fusebox_servers` tool to see them (pass `server` to start one and list its tools) and `fusebox_call` to call a tool. Switch with `fusebox mcp direct ID` / `fusebox mcp hub ID`, then `fusebox reconcile`.

Skills (sources in `{hub}`):
- Every Skill is either resident (in your skill list) or on demand. When no listed skill fits a task, run `fusebox skills search WORDS`; if one fits, run `fusebox skills show NAME` and follow it.
- Inspect with `fusebox skills status`; `fusebox skills usage` shows what each agent actually used; `fusebox advise` suggests what to make resident or on demand.
- Make a skill resident with `fusebox skills enable PROFILE SKILL` (PROFILE is `global` or a project), on demand with `fusebox skills disable PROFILE SKILL`, or resident only in one project with `fusebox skills move SKILL --to PROJECT`. These sync immediately.
- Apply `fusebox advise` suggestions only after the user approves them. If unexpected global Skills appear, report them without changing activation and do not block unrelated tasks.
- Third-party Skills live once under `vendor/` with a locked commit; self-authored ones in Git-managed `own/` or their own repos. `{global_skills}` is generated output: never copy Skills there by hand. Do not run broad global installs such as `skills add ... -g --all` unless asked.
- Never store credentials in Skill files, Skill references, Skill scripts, registry files, lock files, or audit reports.
"
    )
}

/// Names of everything on demand, for hosts without a native name-only
/// skill listing (Codex, Hermes, DeepSeek Harness). Names alone pick as
/// well as full descriptions at a fraction of the context.
fn catalog_index(paths: &Paths) -> String {
    let resident: Vec<String> = crate::skills::Hub::load(&skill_hub_dir(paths))
        .and_then(|h| crate::skills::enabled_skills(&h, "global"))
        .map(|v| v.into_iter().map(|(n, _)| n).collect())
        .unwrap_or_default();
    let mut names: Vec<String> = crate::skills::catalog(&skill_hub_dir(paths))
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.name)
        .filter(|n| !resident.contains(n))
        .collect();
    names.sort();
    names.dedup();
    let servers: Vec<String> = crate::config::load(&paths.config_file, &paths.secrets_file)
        .map(|c| {
            c.tools
                .iter()
                .filter(|t| t.enabled && t.mode == crate::config::Mode::Hub)
                .map(|t| t.id.trim_start_matches("agent-").replace('-', "_"))
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() && servers.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nOn-demand catalog (names only — load what fits the task):\n");
    if !names.is_empty() {
        out.push_str(&format!("- Skills (`fusebox skills show NAME`): {}\n", names.join(", ")));
    }
    if !servers.is_empty() {
        out.push_str(&format!(
            "- MCP servers (agent-hub `fusebox_servers` / `fusebox_call`): {}\n",
            servers.join(", ")
        ));
    }
    out.push_str("- Unsure which? `fusebox find WORDS` (English or Chinese) ranks both.\n");
    out
}

pub fn codex(paths: &Paths) -> String {
    core_policy(paths)
        + &catalog_index(paths)
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
        + &catalog_index(paths)
        + "
Hermes-specific rule:
- Hermes provider switching may be handled elsewhere, but MCP credentials and tool secrets remain managed by Fusebox.
"
}

pub fn deepseek(paths: &Paths) -> String {
    core_policy(paths)
        + &catalog_index(paths)
        + "
DeepSeek Harness-specific rule:
- MCP servers reach dsh through the Fusebox block in `~/.dsh/cordis.patch.yml`; do not edit that block by hand, use `fusebox mcp ...` and `fusebox reconcile`.
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
    if has("deepseek") {
        let current = read_text(&paths.dsh_instructions)?;
        out.push((
            "instructions.deepseek",
            paths.dsh_instructions.clone(),
            merge_managed_block(&current, &deepseek(paths)),
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
