//! Claude Code projection of the two-state model.
//!
//! Every Skill on disk is linked into `~/.claude/skills`; resident ones list
//! in full, on-demand ones are `name-only` in `skillOverrides`, so Claude
//! always sees every name (an index that costs a few tokens each) and loads
//! the body when it picks one.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::paths::Paths;
use crate::skills;
use crate::util::{Result, json_ordered, read_text, write_if_changed};

pub const HOOK_MARKER: &str = " hook claude-prompt";

pub struct Plan {
    pub skills_dir: PathBuf,
    pub settings: PathBuf,
    /// name → skill directory
    pub links: BTreeMap<String, PathBuf>,
    pub resident: Vec<String>,
}

fn link_target(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_link(path).ok()?;
    let abs = if raw.is_absolute() { raw } else { path.parent()?.join(raw) };
    Some(crate::util::resolve_lenient(&abs))
}

pub fn plan(paths: &Paths, hub_root: &Path) -> Result<Plan> {
    let mut links: BTreeMap<String, PathBuf> = BTreeMap::new();
    let catalog = skills::catalog(hub_root)?;
    // Resident copies win when the same name exists in several sources.
    for e in catalog.iter().filter(|e| !e.profiles.is_empty()).chain(catalog.iter().filter(|e| e.profiles.is_empty())) {
        if let Some(dir) = e.skill_md.parent() {
            links.entry(e.name.clone()).or_insert_with(|| dir.to_path_buf());
        }
    }
    // The shared global root also carries the OpenAI official baseline.
    let global_root = paths.user_home.join(".agents").join("skills");
    if let Ok(rd) = fs::read_dir(&global_root) {
        for entry in rd.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "README.md" {
                continue;
            }
            if let Some(target) = link_target(&entry.path()) {
                links.entry(name).or_insert(target);
            }
        }
    }
    let hub = skills::Hub::load(hub_root)?;
    let resident = skills::enabled_skills(&hub, "global")?.into_iter().map(|(n, _)| n).collect();
    Ok(Plan {
        skills_dir: paths.user_home.join(".claude").join("skills"),
        settings: paths.user_home.join(".claude").join("settings.json"),
        links,
        resident,
    })
}

fn desired_settings(plan: &Plan, current: &str) -> Result<String> {
    let mut root: Map<String, Value> = if current.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(current).map_err(|e| format!("malformed ~/.claude/settings.json: {e}"))?
    };
    // skillOverrides: on-demand names are name-only; resident and unknown
    // names are left to the user.
    let mut overrides = root.get("skillOverrides").and_then(Value::as_object).cloned().unwrap_or_default();
    // name-only entries for Skills that left the catalog are stale.
    overrides.retain(|name, v| v.as_str() != Some("name-only") || plan.links.contains_key(name));
    for name in plan.links.keys() {
        if plan.resident.contains(name) {
            if overrides.get(name).and_then(Value::as_str) == Some("name-only") {
                overrides.remove(name);
            }
        } else {
            overrides.insert(name.clone(), json!("name-only"));
        }
    }
    root.insert("skillOverrides".into(), Value::Object(overrides));
    // Fusebox does not hook prompts: per-prompt hints were mostly noise
    // (~30% right on real prompts). Remove the entry earlier versions added
    // and drop containers left empty.
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        if let Some(arr) = hooks.get_mut("UserPromptSubmit").and_then(Value::as_array_mut) {
            arr.retain(|group| {
                !group["hooks"]
                    .as_array()
                    .is_some_and(|hs| hs.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(HOOK_MARKER))))
            });
            if arr.is_empty() {
                hooks.remove("UserPromptSubmit");
            }
        }
        if hooks.is_empty() {
            root.remove("hooks");
        }
    }
    Ok(json_ordered(&Value::Object(root)) + "\n")
}

/// Human-readable differences between disk and the plan (empty = in sync).
pub fn diff(plan: &Plan) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if plan.skills_dir.is_symlink() {
        out.push(format!("{} is a shared symlink; becomes a Fusebox-managed directory", plan.skills_dir.display()));
    } else {
        let missing = plan
            .links
            .iter()
            .filter(|(name, target)| link_target(&plan.skills_dir.join(name)).as_ref() != Some(*target))
            .count();
        if missing > 0 {
            out.push(format!("{missing} skill link(s) to add or fix"));
        }
    }
    let current = read_text(&plan.settings)?;
    if desired_settings(plan, &current)? != current {
        out.push("skillOverrides differ".into());
    }
    Ok(out)
}

pub fn apply(plan: &Plan, backup_dir: &Path) -> Result<usize> {
    let mut changed = 0;
    if plan.skills_dir.is_symlink() {
        fs::remove_file(&plan.skills_dir).map_err(|e| e.to_string())?;
        changed += 1;
    }
    fs::create_dir_all(&plan.skills_dir).map_err(|e| e.to_string())?;
    for (name, target) in &plan.links {
        let link = plan.skills_dir.join(name);
        if link_target(&link).as_ref() == Some(target) {
            continue;
        }
        if link.is_symlink() {
            fs::remove_file(&link).map_err(|e| e.to_string())?;
        } else if link.exists() {
            continue; // a real directory the user put there: leave it
        }
        symlink(target, &link).map_err(|e| format!("failed to link {}: {e}", link.display()))?;
        changed += 1;
    }
    // Drop links Fusebox no longer knows about (never real directories).
    if let Ok(rd) = fs::read_dir(&plan.skills_dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_symlink() && !plan.links.contains_key(&name) && !name.starts_with('.') {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                changed += 1;
            }
        }
    }
    let current = read_text(&plan.settings)?;
    let desired = desired_settings(plan, &current)?;
    changed += usize::from(write_if_changed(&plan.settings, &desired, None, Some(backup_dir))?.changed);
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_keep_user_keys_and_own_one_hook() {
        let plan = Plan {
            skills_dir: PathBuf::from("/x"),
            settings: PathBuf::from("/x/settings.json"),
            links: [("a".to_string(), PathBuf::from("/a")), ("fusebox".to_string(), PathBuf::from("/f"))]
                .into_iter()
                .collect(),
            resident: vec!["fusebox".into()],
        };
        let current = r#"{"theme":"dark","skillOverrides":{"mine":"off","fusebox":"name-only","gone":"name-only"},"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"other"}]}]}}"#;
        let out = desired_settings(&plan, current).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["skillOverrides"], json!({"mine": "off", "a": "name-only"}));
        // The user's own hook stays; nothing of Fusebox's is added.
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        // Idempotent: a second pass changes nothing.
        assert_eq!(desired_settings(&plan, &out).unwrap(), out);
    }
}
