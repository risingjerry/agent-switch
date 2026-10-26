//! `find`: one ranked catalog of Skills and MCP servers for a task.
//!
//! BM25 over name (3x) and description, widened with zh↔en synonyms and the
//! hub's `config/aliases.json` (extra phrases people use for an item), plus
//! a prior from past use in this project and overall. The item list is
//! cached so a lookup stays well under 50 ms.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config::{Config, Mode};
use crate::hub;
use crate::paths::Paths;
use crate::skills;
use crate::usage;

const CACHE_VERSION: u64 = 1;
/// Usage changes every session; a slightly stale prior is fine.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Git root (or the directory itself) for attributing past use to "this project".
pub fn project_root(dir: &Path) -> PathBuf {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return d.to_path_buf();
        }
        cur = d.parent();
    }
    dir.to_path_buf()
}

/// Chinese ↔ English terms people use interchangeably for the same tool;
/// the query is widened with the other side so either language matches.
const SYNONYMS: &[(&str, &str)] = &[
    ("手机", "iphone phone mobile"),
    ("截图", "screenshot screen"),
    ("屏幕", "screen"),
    ("点击", "tap click"),
    ("画图", "diagram draw canvas"),
    ("图表", "chart diagram"),
    ("生图", "image generate"),
    ("配图", "image illustration"),
    ("图片", "image"),
    ("公众号", "wechat official account"),
    ("微信", "wechat"),
    ("小红书", "xiaohongshu xhs"),
    ("抖音", "douyin"),
    ("推特", "twitter x"),
    ("飞书", "lark feishu"),
    ("邮件", "email mail"),
    ("邮箱", "email mail"),
    ("部署", "deploy release"),
    ("上线", "deploy release"),
    ("发布", "publish release"),
    ("搜索", "search"),
    ("调研", "research"),
    ("视频", "video"),
    ("剪辑", "edit video jianying"),
    ("剪映", "jianying"),
    ("逆向", "reverse engineer"),
    ("反编译", "decompile reverse"),
    ("翻译", "translate"),
    ("文章", "article"),
    ("写作", "writing"),
    ("退款", "refund"),
    ("充值", "topup recharge"),
    ("对账", "reconciliation"),
    ("价格", "price pricing"),
    ("公告", "announcement"),
    ("通知", "notify notification"),
    ("测试", "test"),
    ("代码审查", "code review"),
    ("提交", "commit"),
    ("设计", "design"),
    ("前端", "frontend ui"),
    ("清理", "cleanup clean"),
    ("磁盘", "disk"),
    ("密钥", "secret key"),
    ("支付宝", "alipay"),
    ("转写", "transcribe transcript"),
    ("录音", "audio recording"),
];

fn widen_query(query: &str) -> String {
    let lower = query.to_lowercase();
    let mut out = query.to_string();
    for (zh, en) in SYNONYMS {
        if query.contains(zh) {
            out.push(' ');
            out.push_str(en);
        }
        if en.split(' ').any(|w| lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|q| q == w)) {
            out.push(' ');
            out.push_str(zh);
        }
    }
    out
}

/// How many on-demand Skills carry a one-line description in agents' name
/// lists. Measured on 112 real prompts: names plus the 30 most used with a
/// line picks better than names alone (and than a line for every Skill) at
/// a third of the all-described size.
pub const DESCRIBED: usize = 30;

/// First sentence of a description, at most 110 characters.
pub fn one_line(description: &str) -> String {
    let flat = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = flat.find(|c| matches!(c, '。' | '；' | ';')).map(|i| i + 3).unwrap_or(flat.len());
    let end = flat.find(". ").map_or(end, |i| end.min(i + 1));
    flat[..end.min(flat.len())].trim_end_matches(['；', ';']).chars().take(110).collect()
}

/// The on-demand Skills used most in the last 90 days (at least twice),
/// with a one-line description each, most used first.
pub fn often_used(paths: &Paths, hub_root: &Path) -> Vec<(String, String)> {
    let Ok(stats) = usage::collect(paths, 90) else { return Vec::new() };
    let mut seen = BTreeSet::new();
    let mut rows: Vec<(usize, String, String)> = skills::catalog(hub_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.profiles.is_empty() && seen.insert(e.name.clone()))
        .map(|e| {
            let dir = e.skill_md.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(String::from);
            (stats.lookup(&e.name, dir.as_deref()).total(), one_line(&e.description), e.name)
        })
        .filter(|(n, _, _)| *n >= 2)
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)));
    rows.into_iter().take(DESCRIBED).map(|(_, d, n)| (n, d)).collect()
}

/// Extra phrases per Skill name or MCP id, kept in the Skill Hub so they
/// survive upstream updates of vendor Skills.
pub fn aliases_path(hub_root: &Path) -> PathBuf {
    hub_root.join("config").join("aliases.json")
}

fn load_aliases(hub_root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(aliases_path(hub_root))
        .ok()
        .and_then(|t| serde_json::from_str::<BTreeMap<String, Value>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some((k, s)),
            Value::Array(a) => Some((k, a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))),
            _ => None,
        })
        .collect()
}

struct Item {
    name: String,
    text: String,
    payload: Value,
    cwds: BTreeMap<String, usize>,
    total: usize,
}

pub struct Finder {
    items: Vec<Item>,
    index: skills::Bm25,
}

fn mtime(p: &Path) -> u128 {
    fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())
}

fn build(paths: &Paths, hub_root: &Path, config: &Config) -> Vec<Item> {
    let stats = usage::collect(paths, 90).ok();
    let aliases = load_aliases(hub_root);
    let use_of = |u: Option<usage::SkillUse>| u.map(|u| (u.cwds.clone(), u.total())).unwrap_or_default();
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for e in skills::catalog(hub_root).unwrap_or_default() {
        if !seen.insert(e.name.clone()) {
            continue;
        }
        let u = stats
            .as_ref()
            .map(|s| s.lookup(&e.name, e.skill_md.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str())));
        let (cwds, total) = use_of(u);
        let extra = aliases.get(&e.name).map(String::as_str).unwrap_or("");
        items.push(Item {
            name: e.name.replace('-', " "),
            text: format!("{} {extra}", e.description),
            payload: json!({"kind": "skill", "name": e.name, "description": e.description,
                   "resident": !e.profiles.is_empty(), "use": format!("fusebox skills show {}", e.name)}),
            cwds,
            total,
        });
    }
    for tool in config.tools.iter().filter(|t| t.enabled) {
        let tools: Vec<(String, String)> = hub::cached_manifest(paths, &tool.id)
            .and_then(|m| m["tools"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|t| {
                (t["name"].as_str().unwrap_or("").to_string(), t["description"].as_str().unwrap_or("").to_string())
            })
            .collect();
        let short = tool.id.trim_start_matches("agent-").replace('-', "_");
        let extra = aliases.get(&tool.id).or_else(|| aliases.get(&tool.name)).map(String::as_str).unwrap_or("");
        let text = format!(
            "{} {extra} {}",
            tool.description.as_str().unwrap_or(""),
            tools
                .iter()
                .map(|(n, d)| format!("{n} {}", d.chars().take(200).collect::<String>()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let usage = if tool.mode == Mode::Hub {
            format!(
                "agent-hub MCP: fusebox_servers {{\"server\": \"{short}\"}} starts it and lists its tools; then fusebox_call"
            )
        } else {
            format!("resident: call its tools directly as mcp__{}__<tool>", tool.id)
        };
        let (cwds, total) = use_of(stats.as_ref().and_then(|s| s.mcps.get(&tool.id).cloned()));
        items.push(Item {
            name: format!("{} {}", tool.name, short.replace('_', " ")),
            text,
            payload: json!({"kind": "mcp", "name": tool.name, "server": short, "description": tool.description,
                   "resident": tool.mode == Mode::Direct, "tools": tools.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
                   "use": usage}),
            cwds,
            total,
        });
    }
    items
}

impl Finder {
    /// The catalog, from cache when nothing it depends on changed.
    pub fn load(paths: &Paths, hub_root: &Path, config_path: &Path, config: &Config) -> Finder {
        let stamp = json!([
            CACHE_VERSION,
            mtime(config_path).to_string(),
            mtime(&hub_root.join("config").join("registry.json")).to_string(),
            mtime(&hub_root.join("profiles")).to_string(),
            mtime(&aliases_path(hub_root)).to_string(),
            mtime(&paths.cache_dir.join("tools")).to_string(),
        ]);
        let cache = paths.cache_dir.join("find-index.json");
        let fresh = fs::metadata(&cache)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < CACHE_TTL);
        let cached = fresh
            .then(|| fs::read_to_string(&cache).ok())
            .flatten()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|v| v["stamp"] == stamp)
            .and_then(|v| {
                v["items"].as_array().map(|a| {
                    a.iter()
                        .map(|i| Item {
                            name: i["n"].as_str().unwrap_or("").into(),
                            text: i["t"].as_str().unwrap_or("").into(),
                            payload: i["p"].clone(),
                            cwds: serde_json::from_value(i["c"].clone()).unwrap_or_default(),
                            total: i["u"].as_u64().unwrap_or(0) as usize,
                        })
                        .collect::<Vec<_>>()
                })
            });
        let items = cached.unwrap_or_else(|| {
            let items = build(paths, hub_root, config);
            let rows: Vec<Value> = items
                .iter()
                .map(|i| json!({"n": i.name, "t": i.text, "p": i.payload, "c": i.cwds, "u": i.total}))
                .collect();
            crate::util::ensure_private_dir(&paths.cache_dir);
            let _ = crate::util::write_if_changed(
                &cache,
                &json!({"stamp": stamp, "items": rows}).to_string(),
                Some(0o600),
                None,
            );
            items
        });
        let docs: Vec<(String, String)> = items.iter().map(|i| (i.name.clone(), i.text.clone())).collect();
        Finder { index: skills::Bm25::new(&docs), items }
    }

    /// Best matches for `query`. `root` is the project to favour (None: no
    /// usage prior at all, for measuring the text match alone).
    pub fn hits(&self, query: &str, root: Option<&Path>, limit: usize) -> Vec<Value> {
        let query = widen_query(query);
        let mut scored: Vec<(f64, Value)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let text = self.index.score(i, &query);
                if text <= 0.0 {
                    return None;
                }
                let prior = root.map_or(0.0, |root| {
                    let here: usize =
                        item.cwds.iter().filter(|(c, _)| Path::new(c).starts_with(root)).map(|(_, n)| n).sum();
                    1.5 * (here as f64).ln_1p() + 0.3 * (item.total as f64).ln_1p()
                });
                let score = text + prior;
                let mut v = item.payload.clone();
                v["score"] = json!((score * 100.0).round() / 100.0);
                v["match"] = json!((text * 100.0).round() / 100.0);
                Some((score, v))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(limit);
        scored.into_iter().map(|(_, v)| v).collect()
    }
}
