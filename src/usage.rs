//! `skills usage` / `skills prune`: which skills each agent actually used,
//! counted from local session transcripts, with an incremental cache.
//!
//! Claude Code: `~/.claude/projects/**/*.jsonl` — `Skill` tool calls and
//! `Read` calls on a `<name>/SKILL.md`. Codex: `~/.codex/sessions/**/*.jsonl` —
//! `function_call` / `custom_tool_call` arguments that reference a
//! `<name>/SKILL.md` (the injected skill list is not a tool call, so it is
//! ignored).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex_lite::Regex;
use serde_json::{Map, Value, json};

use crate::paths::Paths;
use crate::util::{Result, write_if_changed};

const CACHE_VERSION: u64 = 4;

static SKILL_MD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([A-Za-z0-9_.:-]+)/SKILL\.md").unwrap());
static CLAUDE_SKILL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""name":"Skill","input":\{"skill":"([^"]+)""#).unwrap());
/// MCP tool calls: `mcp__<server>__<tool>` (Claude Code, DeepSeek Harness)
/// and `"server":"<server>","tool":` (Codex).
static MCP_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:tool_use|tool-call)","id":"[^"]*","name":"mcp__([^"]+)""#).unwrap());
static CODEX_MCP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""server":"([^"]+)","tool":""#).unwrap());

/// Recorded as `mcp:<server>`; tools served through the hub are recorded as
/// `mcp:agent-hub/<short-id>`.
fn mcp_key(qualified: &str) -> Option<String> {
    let mut parts = qualified.splitn(3, "__");
    let server = parts.next()?;
    if server == "agent-hub" {
        return parts.next().map(|short| format!("mcp:agent-hub/{short}"));
    }
    Some(format!("mcp:{server}"))
}

static DSH_SKILL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""name":"skill","arguments":"\{\\"name\\":\s*\\"([^"\\]+)\\""#).unwrap());
static CLAUDE_READ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""name":"Read","input":\{"file_path":"[^"]*?([A-Za-z0-9_.:-]+)/SKILL\.md""#).unwrap()
});

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Agent {
    Claude,
    Codex,
    DeepSeek,
}

impl Agent {
    fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::DeepSeek => "deepseek",
        }
    }
}

/// Byte substring search; the needle is short, the haystack is a whole file.
fn find_all<'a>(hay: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    let first = needle[0];
    let mut pos = 0;
    std::iter::from_fn(move || {
        while pos + needle.len() <= hay.len() {
            let offset = hay[pos..hay.len() - needle.len() + 1].iter().position(|&b| b == first)?;
            let at = pos + offset;
            pos = at + 1;
            if &hay[at..at + needle.len()] == needle {
                return Some(at);
            }
        }
        None
    })
}

/// Each distinct line that contains `needle`.
fn lines_with<'a>(hay: &'a [u8], needle: &'a [u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut last_end = 0;
    for at in find_all(hay, needle) {
        if at < last_end {
            continue;
        }
        let start = hay[..at].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let end = hay[at..].iter().position(|&b| b == b'\n').map_or(hay.len(), |i| at + i);
        out.push(&hay[start..end]);
        last_end = end;
    }
    out
}

/// First `"cwd":"…"` in a session log: Claude Code, Codex and DeepSeek
/// Harness all record the working directory near the top.
pub fn session_cwd(bytes: &[u8]) -> String {
    let needle = b"\"cwd\":\"";
    let Some(at) = find_all(bytes, needle).next() else { return String::new() };
    let rest = &bytes[at + needle.len()..];
    let end = rest.iter().position(|&b| b == b'"').unwrap_or(0);
    String::from_utf8_lossy(&rest[..end]).into_owned()
}

/// On-demand skills are loaded with `fusebox skills show NAME`; count those
/// like direct invocations.
static SHOW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"fusebox skills show ([A-Za-z0-9_.:-]+)").unwrap());

pub fn scan_file(agent: Agent, bytes: &[u8]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    match agent {
        Agent::Claude => {
            for line in lines_with(bytes, b"\"name\":\"Skill\"") {
                let line = String::from_utf8_lossy(line);
                found.extend(CLAUDE_SKILL_RE.captures_iter(&line).map(|c| c[1].to_string()));
            }
            for line in lines_with(bytes, b"/SKILL.md") {
                let line = String::from_utf8_lossy(line);
                if line.contains("\"type\":\"tool_use\"") {
                    found.extend(CLAUDE_READ_RE.captures_iter(&line).map(|c| c[1].to_string()));
                }
            }
        }
        Agent::Codex => {
            for line in lines_with(bytes, b"/SKILL.md") {
                if find_all(line, b"_call\"").next().is_none() {
                    continue;
                }
                let Ok(value) = serde_json::from_slice::<Value>(line) else { continue };
                let payload = &value["payload"];
                if !matches!(payload["type"].as_str(), Some("function_call" | "custom_tool_call")) {
                    continue;
                }
                for field in ["arguments", "input"] {
                    if let Some(text) = payload[field].as_str() {
                        found.extend(SKILL_MD_RE.captures_iter(text).map(|c| c[1].to_string()));
                    }
                }
            }
        }
        Agent::DeepSeek => {
            for line in lines_with(bytes, b"\"name\":\"skill\",\"arguments\"") {
                let line = String::from_utf8_lossy(line);
                found.extend(DSH_SKILL_RE.captures_iter(&line).map(|c| c[1].to_string()));
            }
        }
    }
    for line in lines_with(bytes, b"fusebox skills show ") {
        let line = String::from_utf8_lossy(line);
        let is_call =
            ["\"tool_use\"", "\"tool-call\"", "function_call", "custom_tool_call"].iter().any(|m| line.contains(m));
        if is_call {
            found.extend(SHOW_RE.captures_iter(&line).map(|c| c[1].to_string()));
        }
    }
    found.retain(|n| !n.is_empty() && n != "." && n != "..");
    match agent {
        Agent::Codex => {
            for line in lines_with(bytes, b"\"server\":\"") {
                let line = String::from_utf8_lossy(line);
                found.extend(CODEX_MCP_RE.captures_iter(&line).map(|c| format!("mcp:{}", &c[1])));
            }
        }
        Agent::Claude | Agent::DeepSeek => {
            for line in lines_with(bytes, b"\"name\":\"mcp__") {
                let line = String::from_utf8_lossy(line);
                found.extend(MCP_NAME_RE.captures_iter(&line).filter_map(|c| mcp_key(&c[1])));
            }
        }
    }
    found
}

/// Session logs are plain JSONL, except DeepSeek Harness which zstd-compresses them.
fn read_log(path: &Path) -> std::io::Result<Vec<u8>> {
    let raw = fs::read(path)?;
    if path.extension().is_some_and(|x| x == "zstd") {
        use std::io::Read;
        // dsh appends one zstd frame per write; decode every frame.
        let mut out = Vec::new();
        let mut input: &[u8] = &raw;
        while !input.is_empty() {
            let mut frame = ruzstd::decoding::StreamingDecoder::new(&mut input).map_err(std::io::Error::other)?;
            frame.read_to_end(&mut out)?;
        }
        return Ok(out);
    }
    Ok(raw)
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        match e.file_type() {
            Ok(ft) if ft.is_dir() => collect_jsonl(&p, out),
            Ok(ft) if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl" || x == "zstd") => out.push(p),
            _ => {}
        }
    }
}

struct FileEntry {
    agent: Agent,
    size: u64,
    mtime: i64,
    skills: Vec<String>,
    /// Working directory of the session, used to attribute use to projects.
    cwd: String,
}

fn load_cache(path: &Path) -> BTreeMap<String, FileEntry> {
    let Ok(text) = fs::read_to_string(path) else { return BTreeMap::new() };
    let Ok(value) = serde_json::from_str::<Value>(&text) else { return BTreeMap::new() };
    if value["version"].as_u64() != Some(CACHE_VERSION) {
        return BTreeMap::new();
    }
    let Some(files) = value["files"].as_object() else { return BTreeMap::new() };
    files
        .iter()
        .filter_map(|(k, v)| {
            Some((
                k.clone(),
                FileEntry {
                    agent: match v["agent"].as_str() {
                        Some("codex") => Agent::Codex,
                        Some("deepseek") => Agent::DeepSeek,
                        _ => Agent::Claude,
                    },
                    size: v["size"].as_u64()?,
                    mtime: v["mtime"].as_i64()?,
                    skills: v["skills"].as_array()?.iter().filter_map(|s| s.as_str().map(String::from)).collect(),
                    cwd: v["cwd"].as_str().unwrap_or("").to_string(),
                },
            ))
        })
        .collect()
}

fn save_cache(path: &Path, files: &BTreeMap<String, FileEntry>) -> Result<()> {
    let mut map = Map::new();
    for (k, e) in files {
        map.insert(
            k.clone(),
            json!({"agent": e.agent.key(), "size": e.size, "mtime": e.mtime, "skills": e.skills, "cwd": e.cwd}),
        );
    }
    if let Some(parent) = path.parent() {
        crate::util::ensure_private_dir(parent);
    }
    write_if_changed(path, &json!({"version": CACHE_VERSION, "files": map}).to_string(), Some(0o600), None).map(|_| ())
}

#[derive(Default, Clone)]
pub struct SkillUse {
    pub claude: usize,
    pub codex: usize,
    pub deepseek: usize,
    pub last_used: i64,
    /// Session working directory → sessions that used it there.
    pub cwds: BTreeMap<String, usize>,
    /// Modification time of every session that used it.
    pub times: Vec<i64>,
}

impl SkillUse {
    pub fn total(&self) -> usize {
        self.claude + self.codex + self.deepseek
    }

    /// Sessions on or after a `YYYY-MM-DD` date.
    pub fn since(&self, date: &str) -> usize {
        self.times.iter().filter(|t| date_of(**t).as_str() >= date).count()
    }
}

pub struct UsageStats {
    pub days: u64,
    pub sessions: BTreeMap<&'static str, usize>,
    pub skills: BTreeMap<String, SkillUse>,
    /// MCP server id (or `agent-hub/<short>`) → sessions that called it.
    pub mcps: BTreeMap<String, SkillUse>,
    pub scanned: usize,
    pub cached: usize,
}

impl UsageStats {
    /// Usage for a skill, matching its name or the last path component
    /// (plugin skills are logged as `plugin:name`, files as `<dir>/SKILL.md`).
    pub fn lookup(&self, name: &str, dir: Option<&str>) -> SkillUse {
        let mut total = SkillUse::default();
        for (key, u) in &self.skills {
            let short = key.rsplit(':').next().unwrap_or(key);
            if key == name || dir.is_some_and(|d| key == d || short == d) {
                total.claude += u.claude;
                total.codex += u.codex;
                total.deepseek += u.deepseek;
                total.last_used = total.last_used.max(u.last_used);
                for (k, v) in &u.cwds {
                    *total.cwds.entry(k.clone()).or_default() += v;
                }
                total.times.extend(&u.times);
            }
        }
        total
    }
}

pub fn collect(paths: &Paths, days: u64) -> Result<UsageStats> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(days * 86_400))
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    let roots = [
        (Agent::Claude, paths.user_home.join(".claude").join("projects")),
        (Agent::Codex, paths.user_home.join(".codex").join("sessions")),
        (Agent::DeepSeek, paths.dsh_home.join("sessions")),
    ];
    let mut files: Vec<(Agent, PathBuf, u64, i64)> = Vec::new();
    for (agent, root) in &roots {
        let mut found = Vec::new();
        collect_jsonl(root, &mut found);
        for p in found {
            let Ok(meta) = fs::metadata(&p) else { continue };
            if meta.mtime() >= cutoff {
                files.push((*agent, p, meta.len(), meta.mtime()));
            }
        }
    }
    let cache_path = paths.cache_dir.join("skill-usage.json");
    let cache = load_cache(&cache_path);
    let mut fresh: BTreeMap<String, FileEntry> = BTreeMap::new();
    let mut todo = Vec::new();
    for (agent, path, size, mtime) in files {
        let key = path.to_string_lossy().into_owned();
        match cache.get(&key) {
            Some(e) if e.size == size && e.mtime == mtime && e.agent == agent => {
                fresh.insert(key, FileEntry { agent, size, mtime, skills: e.skills.clone(), cwd: e.cwd.clone() });
            }
            _ => todo.push((agent, path, size, mtime, key)),
        }
    }
    let cached = fresh.len();
    let scanned = todo.len();
    // Bounded parallelism: this machine also runs agents and builds.
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let chunk = todo.len().div_ceil(workers).max(1);
    let results: Vec<(String, FileEntry)> = std::thread::scope(|scope| {
        let handles: Vec<_> = todo
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|(agent, path, size, mtime, key)| {
                            let (skills, cwd) = read_log(path)
                                .map(|b| (scan_file(*agent, &b).into_iter().collect(), session_cwd(&b)))
                                .unwrap_or_default();
                            (key.clone(), FileEntry { agent: *agent, size: *size, mtime: *mtime, skills, cwd })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    fresh.extend(results);
    if scanned > 0 || cached != cache.len() {
        save_cache(&cache_path, &fresh)?;
    }
    let mut stats =
        UsageStats { days, sessions: BTreeMap::new(), skills: BTreeMap::new(), mcps: BTreeMap::new(), scanned, cached };
    for e in fresh.values() {
        *stats.sessions.entry(e.agent.key()).or_default() += 1;
        for name in &e.skills {
            let u = match name.strip_prefix("mcp:") {
                Some(server) => stats.mcps.entry(server.to_string()).or_default(),
                None => stats.skills.entry(name.clone()).or_default(),
            };
            match e.agent {
                Agent::Claude => u.claude += 1,
                Agent::Codex => u.codex += 1,
                Agent::DeepSeek => u.deepseek += 1,
            }
            u.last_used = u.last_used.max(e.mtime);
            *u.cwds.entry(e.cwd.clone()).or_default() += 1;
            u.times.push(e.mtime);
        }
    }
    Ok(stats)
}

/// Characters of `name` + `description` from a SKILL.md frontmatter: what
/// every session pays for having the skill listed.
pub fn listing_chars(skill_md: &Path) -> usize {
    let Ok(text) = fs::read_to_string(skill_md) else { return 0 };
    let Some(body) = text.strip_prefix("---") else { return 0 };
    let front = body.split("\n---").next().unwrap_or("");
    let mut total = 0;
    let mut in_desc = false;
    for line in front.lines() {
        let is_key = line.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && line.contains(':');
        if is_key {
            in_desc = false;
            let (key, value) = line.split_once(':').unwrap();
            if matches!(key.trim(), "name" | "description" | "when_to_use") {
                total += value.trim().chars().count();
                in_desc = key.trim() != "name";
            }
        } else if in_desc {
            total += line.trim().chars().count();
        }
    }
    total
}

pub fn date_of(ts: i64) -> String {
    if ts <= 0 {
        return "-".into();
    }
    crate::skills::now_iso_at(ts as u64)[..10].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_claude_skill_and_read_calls() {
        let log = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"pdf-tools","args":"x"}}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/a/skills/release-notes/SKILL.md"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":"see /x/other/SKILL.md"}}"#,
            "\n"
        );
        let found = scan_file(Agent::Claude, log.as_bytes());
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec!["pdf-tools", "release-notes"]);
    }

    #[test]
    fn scans_codex_tool_calls_only() {
        let log = concat!(
            r#"{"type":"session_meta","payload":{"instructions":"- a: x (file: r0/listed/SKILL.md)"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"function_call","name":"exec","arguments":"{\"cmd\":\"sed -n 1,80p /s/skills/slide-deck/SKILL.md\"}"}}"#,
            "\n"
        );
        let found = scan_file(Agent::Codex, log.as_bytes());
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec!["slide-deck"]);
    }

    #[test]
    fn counts_mcp_servers_per_agent() {
        let claude = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__agent-browser__click","input":{}},{"type":"tool_use","id":"t2","name":"mcp__agent-hub__github__search","input":{}}]}}"#;
        let found = scan_file(Agent::Claude, claude.as_bytes());
        assert!(found.contains("mcp:agent-browser") && found.contains("mcp:agent-hub/github"));
        let codex = r#"{"type":"event_msg","payload":{"type":"mcp_tool_call_end","invocation":{"server":"agent-github","tool":"search_code"}}}"#;
        assert!(scan_file(Agent::Codex, codex.as_bytes()).contains("mcp:agent-github"));
    }

    #[test]
    fn scans_deepseek_skill_tool() {
        let log =
            r#"{"type":"tool-call","id":"c1","name":"skill","arguments":"{\"name\": \"reverse-engineer-anything\"}"}"#;
        let found = scan_file(Agent::DeepSeek, log.as_bytes());
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec!["reverse-engineer-anything"]);
    }

    #[test]
    fn listing_cost_counts_frontmatter() {
        let dir = std::env::temp_dir().join(format!("as-usage-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("SKILL.md");
        fs::write(&f, "---\nname: abc\ndescription: >\n  hello\n  world\nlicense: MIT\n---\nbody").unwrap();
        assert_eq!(listing_chars(&f), 3 + 1 + 5 + 5);
        let _ = fs::remove_dir_all(&dir);
    }
}
