//! Skills: sources, version locks, activation profiles, symlink sync,
//! usage statistics and pruning. Data lives in the Skill Hub directory
//! (`config/registry.json`, `skills.lock.json`, `profiles/*.json`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

use crate::util::{Result, json_ordered, relpath, resolve_lenient};

pub struct Hub {
    pub root: PathBuf,
    pub registry: Map<String, Value>,
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>> {
    let text = fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!("expected JSON object in {}", path.display())),
        Err(e) => Err(format!("invalid JSON in {}: {e}", path.display())),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::util::write_if_changed(path, &(json_ordered(value) + "\n"), None, None).map(|_| ())
}

fn s(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

impl Hub {
    pub fn load(root: &Path) -> Result<Hub> {
        let root = resolve_lenient(root);
        let registry_path = root.join("config").join("registry.json");
        if !registry_path.exists() {
            return Err(format!("missing registry: {}", registry_path.display()));
        }
        Ok(Hub { registry: read_json_object(&registry_path)?, root })
    }

    /// Registry sources sorted by id.
    fn sources(&self) -> Map<String, Value> {
        let mut entries: Vec<(String, Value)> =
            self.registry.get("sources").and_then(Value::as_object).cloned().unwrap_or_default().into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.into_iter().collect()
    }

    fn source(&self, name: &str) -> Result<Map<String, Value>> {
        self.sources().get(name).and_then(Value::as_object).cloned().ok_or_else(|| format!("unknown source: {name}"))
    }

    pub fn source_base(&self, name: &str) -> Result<PathBuf> {
        let src = self.source(name)?;
        match s(src.get("type")) {
            Some("git") => Ok(resolve_lenient(&self.root.join(s(src.get("checkout")).unwrap_or("")))),
            Some("local") => Ok(resolve_lenient(&crate::util::expand_user(s(src.get("path")).unwrap_or("")))),
            other => Err(format!("unsupported source type for {name}: {}", other.unwrap_or("None"))),
        }
    }

    pub fn skill_dir(&self, source: &str, path: &str) -> Result<PathBuf> {
        let src = self.source(source)?;
        let base = self.source_base(source)?;
        Ok(resolve_lenient(&base.join(s(src.get("skillPath")).unwrap_or(".")).join(path)))
    }

    fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    fn profile_path(&self, name: &str) -> Result<PathBuf> {
        let path = self.profiles_dir().join(format!("{name}.json"));
        if path.exists() { Ok(path) } else { Err(format!("missing profile: {}", path.display())) }
    }

    pub fn profile(&self, name: &str) -> Result<Map<String, Value>> {
        read_json_object(&self.profile_path(name)?)
    }

    pub fn profile_names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.profiles_dir())
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn global_root(&self) -> PathBuf {
        let raw =
            s(self.registry.get("globalSkillRoot")).map(String::from).unwrap_or_else(|| "~/.agents/skills".into());
        resolve_lenient(&crate::util::expand_user(&raw))
    }

    fn lock_path(&self) -> PathBuf {
        self.root.join("skills.lock.json")
    }
}

fn profile_skills(profile: &Map<String, Value>) -> Vec<Map<String, Value>> {
    profile
        .get("skills")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_object().cloned()).collect())
        .unwrap_or_default()
}

fn entry_name(item: &Map<String, Value>) -> String {
    s(item.get("name")).unwrap_or("").to_string()
}

fn entry_path(item: &Map<String, Value>) -> String {
    s(item.get("path")).map(String::from).unwrap_or_else(|| entry_name(item))
}

fn ensure_skill(path: &Path) -> Result<()> {
    if path.join("SKILL.md").exists() {
        Ok(())
    } else {
        Err(format!("skill source missing SKILL.md: {}", path.display()))
    }
}

fn git(args: &[&str], cwd: &Path) -> Result<String> {
    let out =
        Command::new("git").args(args).current_dir(cwd).output().map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn now_iso() -> String {
    now_iso_at(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}

pub fn now_iso_at(secs: u64) -> String {
    let (days, rem) = (secs / 86400, secs % 86400);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

// ---- symlink management ----

/// Point `link` at `target` with a relative symlink. Refuses to touch real
/// files, except a directory whose SKILL.md already links to the target.
fn replace_symlink(link: &Path, target: &Path, dry_run: bool) -> Result<&'static str> {
    let target = resolve_lenient(target);
    let parent = resolve_lenient(link.parent().unwrap_or(Path::new(".")));
    let desired = relpath(&target, &parent);
    if link.is_symlink() {
        let raw = fs::read_link(link).map_err(|e| e.to_string())?;
        let current = resolve_lenient(&if raw.is_absolute() { raw.clone() } else { link.parent().unwrap().join(&raw) });
        if current == target && raw == desired {
            return Ok("unchanged");
        }
        if !dry_run {
            fs::remove_file(link).map_err(|e| e.to_string())?;
        }
    } else if link.exists() {
        let skill_link = link.join("SKILL.md");
        if link.is_dir() && skill_link.is_symlink() {
            let raw = fs::read_link(&skill_link).map_err(|e| e.to_string())?;
            let current = resolve_lenient(&if raw.is_absolute() { raw } else { link.join(raw) });
            if current == resolve_lenient(&target.join("SKILL.md")) {
                return Ok("unchanged");
            }
        }
        return Err(format!("refusing to overwrite non-symlink path: {}", link.display()));
    }
    if !dry_run {
        fs::create_dir_all(link.parent().unwrap()).map_err(|e| e.to_string())?;
        symlink(&desired, link).map_err(|e| format!("failed to link {}: {e}", link.display()))?;
    }
    Ok("linked")
}

fn prune_dir(dir: &Path, keep: &BTreeSet<String>, dry_run: bool) -> Result<Vec<PathBuf>> {
    let Ok(rd) = fs::read_dir(dir) else { return Ok(Vec::new()) };
    let mut children: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    children.sort();
    let mut pruned = Vec::new();
    for child in children {
        let name = child.file_name().unwrap().to_string_lossy().into_owned();
        if keep.contains(&name) || !child.is_symlink() {
            continue;
        }
        if !dry_run {
            fs::remove_file(&child).map_err(|e| e.to_string())?;
        }
        pruned.push(child);
    }
    Ok(pruned)
}

fn display_rel(path: &Path, project: &Path) -> String {
    path.strip_prefix(project).map(|p| p.display().to_string()).unwrap_or_else(|_| path.display().to_string())
}

pub fn sync_profile(hub: &Hub, name: &str, dry_run: bool, prune: bool) -> Result<Vec<String>> {
    let profile = hub.profile(name)?;
    let project = resolve_lenient(&crate::util::expand_user(s(profile.get("project")).unwrap_or(".")));
    let active_dir = project.join(s(profile.get("activeDir")).unwrap_or(".agents/skills"));
    let agent_dirs = hub.registry.get("agentSkillDirs").and_then(Value::as_object).cloned().unwrap_or_default();
    let agents: Vec<String> = profile
        .get("agents")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let bridge = |agent: &str| -> Result<PathBuf> {
        s(agent_dirs.get(agent))
            .map(|rel| project.join(rel))
            .ok_or_else(|| format!("profile {name} references unknown agent: {agent}"))
    };
    let items = profile_skills(&profile);
    let keep: BTreeSet<String> = items.iter().map(entry_name).collect();
    let mut events = Vec::new();
    if !dry_run {
        fs::create_dir_all(&active_dir).map_err(|e| e.to_string())?;
    }
    for item in &items {
        let skill = entry_name(item);
        let source_dir = hub.skill_dir(s(item.get("source")).unwrap_or(""), &entry_path(item))?;
        ensure_skill(&source_dir)?;
        let link = active_dir.join(&skill);
        let status = replace_symlink(&link, &source_dir, dry_run)?;
        events.push(format!("{status}: {} -> {}", display_rel(&link, &project), source_dir.display()));
        for agent in &agents {
            let link = bridge(agent)?.join(&skill);
            let status = replace_symlink(&link, &source_dir, dry_run)?;
            events.push(format!("{status}: {} -> {}", display_rel(&link, &project), source_dir.display()));
        }
    }
    if prune {
        let mut dirs = vec![active_dir.clone()];
        for agent in &agents {
            dirs.push(bridge(agent)?);
        }
        for dir in dirs {
            for p in prune_dir(&dir, &keep, dry_run)? {
                events.push(format!("pruned: {}", display_rel(&p, &project)));
            }
        }
    }
    Ok(events)
}

// ---- official baseline + global profile ----

struct GlobalSkill {
    name: String,
    path: PathBuf,
    source: String,
    root: String,
    official: bool,
}

fn find_skill_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            find_skill_files(&p, out);
        } else if e.file_name() == "SKILL.md" {
            out.push(p);
        }
    }
}

fn official_name(root_id: &str, root: &Path, dir: &Path) -> Result<String> {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let base = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if root_id == "openai-system" {
        return Ok(base);
    }
    let Some(idx) = parts.iter().position(|p| p == "skills") else { return Ok(base) };
    let tail = &parts[idx + 1..];
    if tail.is_empty() {
        return Err(format!("cannot derive official Skill name for {}", dir.display()));
    }
    Ok(std::iter::once(parts[0].clone()).chain(tail.iter().cloned()).collect::<Vec<_>>().join(":"))
}

fn official_excluded(hub: &Hub, name: &str) -> bool {
    hub.registry.get("officialGlobalExclude").and_then(Value::as_array).is_some_and(|list| {
        list.iter().filter_map(Value::as_str).any(|e| name == e || (e.ends_with(':') && name.starts_with(e)))
    })
}

fn official_skills(hub: &Hub) -> Result<(Vec<GlobalSkill>, usize)> {
    let mut by_name: BTreeMap<String, GlobalSkill> = BTreeMap::new();
    let mut duplicates = 0;
    let roots = hub.registry.get("officialGlobalRoots").and_then(Value::as_array).cloned().unwrap_or_default();
    for root_cfg in roots {
        let root_id = s(root_cfg.get("id")).unwrap_or("").to_string();
        let root = resolve_lenient(&crate::util::expand_user(s(root_cfg.get("path")).unwrap_or("")));
        if !root.exists() {
            continue;
        }
        let mut files = Vec::new();
        find_skill_files(&root, &mut files);
        files.sort();
        for file in files {
            let dir = resolve_lenient(file.parent().unwrap());
            let name = official_name(&root_id, &root, &dir)?;
            if official_excluded(hub, &name) {
                continue;
            }
            if by_name.contains_key(&name) {
                duplicates += 1;
                continue;
            }
            by_name.insert(
                name.clone(),
                GlobalSkill {
                    name,
                    path: dir,
                    source: "openai-official".into(),
                    root: root_id.clone(),
                    official: true,
                },
            );
        }
    }
    Ok((by_name.into_values().collect(), duplicates))
}

fn global_profile_skills(hub: &Hub) -> Result<Vec<GlobalSkill>> {
    let Ok(profile) = hub.profile("global") else { return Ok(Vec::new()) };
    profile_skills(&profile)
        .iter()
        .map(|item| {
            let source = s(item.get("source")).unwrap_or("").to_string();
            let path = hub.skill_dir(&source, &entry_path(item))?;
            ensure_skill(&path)?;
            Ok(GlobalSkill { name: entry_name(item), path, source, root: "global-profile".into(), official: false })
        })
        .collect()
}

const GLOBAL_README: &str = "# Global Skills Managed By Fusebox\n\nDo not copy third-party or self-authored Skills here by hand. This global root is reserved for the OpenAI official Skill baseline plus explicit `profiles/global.json` entries. Use `fusebox skills sync-global --prune` to restore or refresh it. Use project profiles for non-official Skills.\n";

pub fn sync_global(hub: &Hub, dry_run: bool, prune: bool) -> Result<Vec<String>> {
    let root = hub.global_root();
    let (official, duplicates) = official_skills(hub)?;
    let profile = global_profile_skills(hub)?;
    let official_names: BTreeSet<&str> = official.iter().map(|s| s.name.as_str()).collect();
    let collisions: Vec<&str> =
        profile.iter().map(|s| s.name.as_str()).filter(|n| official_names.contains(n)).collect();
    if !collisions.is_empty() {
        return Err(format!("global profile names conflict with official Skills: {}", collisions.join(", ")));
    }
    let all: Vec<&GlobalSkill> = official.iter().chain(&profile).collect();
    let desired: BTreeSet<&str> = all.iter().map(|s| s.name.as_str()).collect();
    let mut events = Vec::new();
    if !dry_run {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        crate::util::write_if_changed(&root.join("README.md"), GLOBAL_README, None, None)?;
    }
    for skill in &all {
        ensure_skill(&skill.path)?;
        let status = replace_symlink(&root.join(&skill.name), &skill.path, dry_run)?;
        events.push(format!("{status}: {} -> {}", skill.name, skill.path.display()));
    }
    if prune && let Ok(rd) = fs::read_dir(&root) {
        let mut children: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        children.sort();
        for child in children {
            let name = child.file_name().unwrap().to_string_lossy().into_owned();
            // "synced" belongs to Claude's claude.ai skill sync.
            if name == "README.md" || name == "synced" || name.starts_with('.') || desired.contains(name.as_str()) {
                continue;
            }
            if !child.is_symlink() {
                return Err(format!("refusing to prune non-symlink global path: {}", child.display()));
            }
            if !dry_run {
                fs::remove_file(&child).map_err(|e| e.to_string())?;
            }
            events.push(format!("pruned: {name}"));
        }
    }
    if !dry_run {
        let now = now_iso();
        let mut skills = Map::new();
        for skill in &all {
            let (source_type, root_name) = if skill.official {
                ("local-official-cache", skill.root.as_str())
            } else {
                ("skill-hub-global-profile", "global-profile")
            };
            skills.insert(
                skill.name.clone(),
                json!({
                    "source": skill.source, "sourceType": source_type, "root": root_name,
                    "skillPath": resolve_lenient(&skill.path.join("SKILL.md")).to_string_lossy(),
                    "installedAt": now, "updatedAt": now,
                }),
            );
        }
        let lock = json!({
            "version": 3, "managedBy": "fusebox", "policy": "openai-official-plus-global-profile",
            "updatedAt": now, "skills": skills,
        });
        write_json(&root.parent().unwrap().join(".skill-lock.json"), &lock)?;
    }
    events.push(format!("official global Skills: {}", official.len()));
    events.push(format!("profile global Skills: {}", profile.len()));
    if duplicates > 0 {
        events.push(format!("deduped duplicates: {duplicates}"));
    }
    Ok(events)
}

// ---- profile edits ----

pub fn enable(hub: &Hub, profile_name: &str, name: &str, source: &str, path: Option<&str>) -> Result<()> {
    let file = hub.profile_path(profile_name)?;
    let mut profile = read_json_object(&file)?;
    let path = path.unwrap_or(name);
    ensure_skill(&hub.skill_dir(source, path)?)?;
    let mut items: Vec<Map<String, Value>> =
        profile_skills(&profile).into_iter().filter(|i| entry_name(i) != name).collect();
    let mut entry = Map::new();
    entry.insert("name".into(), json!(name));
    entry.insert("source".into(), json!(source));
    entry.insert("path".into(), json!(path));
    // Recorded so `prune` gives a newly enabled skill time to be used.
    entry.insert("enabledAt".into(), json!(&now_iso()[..10]));
    items.push(entry);
    items.sort_by_key(entry_name);
    profile.insert("skills".into(), Value::Array(items.into_iter().map(Value::Object).collect()));
    write_json(&file, &Value::Object(profile))
}

/// Returns whether the skill was enabled before.
pub fn disable(hub: &Hub, profile_name: &str, name: &str) -> Result<bool> {
    let file = hub.profile_path(profile_name)?;
    let mut profile = read_json_object(&file)?;
    let before = profile_skills(&profile);
    let after: Vec<Value> = before.iter().filter(|i| entry_name(i) != name).cloned().map(Value::Object).collect();
    let removed = after.len() != before.len();
    profile.insert("skills".into(), Value::Array(after));
    write_json(&file, &Value::Object(profile))?;
    Ok(removed)
}

// ---- git sources ----

pub fn fetch(hub: &Hub, only: &[String]) -> Result<Vec<String>> {
    let sources = hub.sources();
    let names: Vec<String> = if only.is_empty() {
        sources.iter().filter(|(_, v)| s(v.get("type")) == Some("git")).map(|(k, _)| k.clone()).collect()
    } else {
        only.to_vec()
    };
    let lock_path = hub.lock_path();
    let mut lock = if lock_path.exists() { read_json_object(&lock_path)? } else { Map::new() };
    lock.entry("version").or_insert(json!(1));
    let mut entries = lock.get("sources").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut events = Vec::new();
    for name in names {
        let src = hub.source(&name)?;
        if s(src.get("type")) != Some("git") {
            return Err(format!("source is not git: {name}"));
        }
        let checkout = hub.source_base(&name)?;
        let url = s(src.get("url")).ok_or_else(|| format!("source {name} has no url"))?;
        let reference = src
            .get("ref")
            .map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))
            .unwrap_or_else(|| "main".into());
        fs::create_dir_all(checkout.parent().unwrap()).map_err(|e| e.to_string())?;
        if checkout.exists() {
            git(&["fetch", "--tags", "--prune"], &checkout)?;
        } else {
            git(&["clone", url, &checkout.to_string_lossy()], &hub.root)?;
        }
        git(&["checkout", &reference], &checkout).map_err(|e| format!("failed to checkout {name}@{reference}: {e}"))?;
        if reference == "main" || reference == "master" {
            let _ = git(&["pull", "--ff-only"], &checkout);
        }
        let full = git(&["rev-parse", "HEAD"], &checkout)?;
        let short = git(&["rev-parse", "--short", "HEAD"], &checkout)?;
        events.push(format!("{name}: {short} ({reference})"));
        entries.insert(
            name.clone(),
            json!({"source": name, "ref": reference, "revision": full, "shortRevision": short, "updatedAt": now_iso()}),
        );
    }
    lock.insert("sources".into(), Value::Object(entries));
    let mut keys: Vec<_> = lock.into_iter().collect();
    keys.sort_by(|a, b| a.0.cmp(&b.0));
    write_json(&lock_path, &Value::Object(keys.into_iter().collect()))?;
    Ok(events)
}

pub fn promote(hub: &Hub, source: &str, reference: &str) -> Result<()> {
    let path = hub.root.join("config").join("registry.json");
    let mut registry = read_json_object(&path)?;
    let src = registry
        .get_mut("sources")
        .and_then(Value::as_object_mut)
        .and_then(|m| m.get_mut(source))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("unknown source: {source}"))?;
    src.insert("ref".into(), json!(reference));
    write_json(&path, &Value::Object(registry))
}

// ---- status / report ----

pub fn status_lines(hub: &Hub, agent_home: &Path) -> Vec<String> {
    let mut lines = vec![
        format!("Skill Hub: {}", hub.root.display()),
        format!("Fusebox: {}", agent_home.display()),
        String::new(),
        "Sources:".into(),
    ];
    for (name, src) in hub.sources() {
        let base = hub.source_base(&name).unwrap_or_default();
        let kind = s(src.get("type")).unwrap_or("");
        let state = if base.exists() { "present" } else { "missing" };
        let reference = s(src.get("ref")).unwrap_or("");
        let head = if kind == "git" && base.join(".git").exists() {
            git(&["rev-parse", "--short", "HEAD"], &base).map(|h| format!(" head={h}")).unwrap_or_default()
        } else {
            String::new()
        };
        lines.push(format!("- {name}: {kind} {state} ref={reference}{head} path={}", base.display()));
    }
    lines.push(String::new());
    lines.push("Profiles:".into());
    for name in hub.profile_names() {
        let Ok(p) = hub.profile(&name) else { continue };
        let agents: Vec<&str> = p
            .get("agents")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        lines.push(format!(
            "- {name}: project={} skills={} agents={}",
            s(p.get("project")).unwrap_or("None"),
            profile_skills(&p).len(),
            if agents.is_empty() { "-".into() } else { agents.join(",") }
        ));
    }
    lines
}

const SKIP_DIRS: [&str; 6] = [".git", ".hg", ".svn", "node_modules", "__pycache__", "archives"];

fn discover_paths(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut has_skill = false;
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.path().is_dir();
            if is_dir {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    walk(root, &e.path(), out);
                }
            } else if name == "SKILL.md" {
                has_skill = true;
            }
        }
        if has_skill {
            let rel = dir.strip_prefix(root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            out.insert(if rel.is_empty() { ".".into() } else { rel });
        }
    }
    let mut out = BTreeSet::new();
    if root.is_dir() {
        walk(root, root, &mut out);
    }
    out
}

/// JSON report consumed by the macOS app (`skills --json`).
pub fn report(hub_root: &Path) -> Result<Value> {
    let hub_root = resolve_lenient(hub_root);
    let registry = read_json_object(&hub_root.join("config").join("registry.json")).unwrap_or_default();
    let locks = read_json_object(&hub_root.join("skills.lock.json"))
        .ok()
        .and_then(|m| m.get("sources").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let mut active: BTreeMap<(String, String), (Vec<String>, Option<String>)> = BTreeMap::new();
    let mut profiles = Vec::new();
    let hub = Hub { root: hub_root.clone(), registry: registry.clone() };
    for name in hub.profile_names() {
        let Ok(p) = hub.profile(&name) else { continue };
        let items = profile_skills(&p);
        profiles.push(json!({"id": name, "project": s(p.get("project")).unwrap_or(""), "skillCount": items.len()}));
        for item in items {
            let key = (s(item.get("source")).unwrap_or("").to_string(), s(item.get("path")).unwrap_or(".").to_string());
            let slot = active.entry(key).or_insert_with(|| (Vec::new(), s(item.get("name")).map(String::from)));
            slot.0.push(name.clone());
        }
    }
    let mut sources = Vec::new();
    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();
    for (id, spec) in hub.sources() {
        let Some(spec) = spec.as_object() else { continue };
        let base = if s(spec.get("type")) == Some("git") {
            hub_root.join(s(spec.get("checkout")).unwrap_or(""))
        } else {
            crate::util::expand_user(s(spec.get("path")).unwrap_or(""))
        };
        let root = resolve_lenient(&base.join(s(spec.get("skillPath")).unwrap_or(".")));
        let lock = locks.get(&id).and_then(Value::as_object).cloned().unwrap_or_default();
        let source_type = s(spec.get("type")).unwrap_or("local").to_string();
        let reference =
            spec.get("ref").filter(|v| !v.is_null()).map(|v| v.as_str().map(String::from).unwrap_or(v.to_string()));
        let revision = s(lock.get("shortRevision")).map(String::from);
        sources.push(json!({
            "id": id, "type": source_type, "path": root.to_string_lossy(), "ref": reference,
            "revision": revision, "updatedAt": s(lock.get("updatedAt")), "installed": root.is_dir(),
        }));
        let mut rels = discover_paths(&root);
        rels.extend(active.keys().filter(|(src, _)| *src == id).map(|(_, p)| p.clone()));
        for rel in rels {
            let key = (id.clone(), rel.clone());
            let (mut profs, act_name) = active.get(&key).cloned().unwrap_or_default();
            seen.insert(key);
            profs.sort();
            let global = profs.iter().any(|p| p == "global");
            let absolute = if rel == "." { root.clone() } else { root.join(&rel) };
            let exists = absolute.join("SKILL.md").is_file();
            let status = if !exists {
                "missing"
            } else if global {
                "global"
            } else if !profs.is_empty() {
                "project"
            } else {
                "dormant"
            };
            let fallback = if rel == "." { root.file_name() } else { Path::new(&rel).file_name() }
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            skills.push(json!({
                "id": format!("{id}:{rel}"), "name": act_name.unwrap_or(fallback), "source": id, "path": rel,
                "absolutePath": absolute.to_string_lossy(), "sourceType": source_type, "ref": reference,
                "revision": revision, "profiles": profs, "globalActive": global, "status": status, "exists": exists,
            }));
        }
    }
    for (key, (mut profs, act_name)) in active {
        if seen.contains(&key) {
            continue;
        }
        profs.sort();
        let fallback = Path::new(&key.1).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        skills.push(json!({
            "id": format!("{}:{}", key.0, key.1), "name": act_name.unwrap_or(fallback), "source": key.0, "path": key.1,
            "absolutePath": "", "sourceType": "missing", "ref": null, "revision": null,
            "globalActive": profs.iter().any(|p| p == "global"), "profiles": profs, "status": "missing", "exists": false,
        }));
    }
    let rank = |v: &Value| {
        let st = v["status"].as_str().unwrap_or("");
        (
            st != "global",
            st != "project",
            v["name"].as_str().unwrap_or("").to_lowercase(),
            v["source"].as_str().unwrap_or("").to_string(),
        )
    };
    skills.sort_by_key(rank);
    let dormant = skills.iter().filter(|v| v["status"] == "dormant").count();
    Ok(json!({
        "hubPath": hub_root.to_string_lossy(), "sources": sources, "profiles": profiles,
        "skills": skills, "dormantCount": dormant,
    }))
}

/// Enabled skills of one profile as (name, SKILL.md path).
pub fn enabled_skills(hub: &Hub, profile_name: &str) -> Result<Vec<(String, PathBuf)>> {
    let profile = hub.profile(profile_name)?;
    profile_skills(&profile)
        .iter()
        .map(|i| Ok((entry_name(i), hub.skill_dir(s(i.get("source")).unwrap_or(""), &entry_path(i))?.join("SKILL.md"))))
        .collect()
}

/// Skills enabled within the last `days` days (by their `enabledAt` date):
/// too new to judge by usage.
pub fn recently_enabled(hub: &Hub, profile_name: &str, days: u64) -> BTreeSet<String> {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let cutoff = now_iso_at(secs.saturating_sub(days * 86_400))[..10].to_string();
    hub.profile(profile_name)
        .map(|p| profile_skills(&p))
        .unwrap_or_default()
        .iter()
        .filter(|i| s(i.get("enabledAt")).is_some_and(|d| d > cutoff.as_str()))
        .map(entry_name)
        .collect()
}

/// Names listed under `keep` in a profile are never pruned.
pub fn profile_keep(hub: &Hub, profile_name: &str) -> BTreeSet<String> {
    hub.profile(profile_name)
        .ok()
        .and_then(|p| p.get("keep").and_then(Value::as_array).cloned())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_752), (2026, 10, 26));
    }

    #[test]
    fn official_names() {
        let root = Path::new("/c/plugins");
        assert_eq!(
            official_name("x", root, Path::new("/c/plugins/notion/3.0.2/skills/setup")).unwrap(),
            "notion:setup"
        );
        assert_eq!(
            official_name("openai-system", root, Path::new("/c/plugins/a/skill-creator")).unwrap(),
            "skill-creator"
        );
    }

    #[test]
    fn sync_links_and_prunes() {
        let base = std::env::temp_dir().join(format!("as-skills-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let hub_root = base.join("hub");
        fs::create_dir_all(hub_root.join("config")).unwrap();
        fs::create_dir_all(hub_root.join("profiles")).unwrap();
        fs::create_dir_all(base.join("src/alpha")).unwrap();
        fs::write(base.join("src/alpha/SKILL.md"), "---\nname: alpha\n---\n").unwrap();
        fs::write(
            hub_root.join("config/registry.json"),
            json!({"sources": {"mine": {"type": "local", "path": base.join("src").to_string_lossy()}},
                   "agentSkillDirs": {"claude": ".claude/skills"}})
            .to_string(),
        )
        .unwrap();
        let project = base.join("proj");
        fs::write(
            hub_root.join("profiles/p.json"),
            json!({"project": project.to_string_lossy(), "agents": ["claude"], "skills": []}).to_string(),
        )
        .unwrap();
        let hub = Hub::load(&hub_root).unwrap();
        enable(&hub, "p", "alpha", "mine", None).unwrap();
        let events = sync_profile(&hub, "p", false, true).unwrap();
        assert!(events[0].starts_with("linked: .agents/skills/alpha"));
        assert!(project.join(".claude/skills/alpha/SKILL.md").exists());
        assert_eq!(sync_profile(&hub, "p", false, true).unwrap()[0].split(':').next(), Some("unchanged"));
        assert!(disable(&hub, "p", "alpha").unwrap());
        let events = sync_profile(&hub, "p", false, true).unwrap();
        assert_eq!(events, vec!["pruned: .agents/skills/alpha", "pruned: .claude/skills/alpha"]);
        let _ = fs::remove_dir_all(&base);
    }
}
