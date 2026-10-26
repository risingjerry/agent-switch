//! `eval`: how well `find` ranks the Skill an agent actually picked, on
//! (prompt → Skill) pairs mined from this machine's Claude Code sessions.
//! Run it after changing descriptions, aliases or the ranking; each run is
//! appended to `~/.cache/fusebox/eval-history.jsonl` so a drop shows up.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::find::{Finder, project_root};
use crate::paths::Paths;

pub struct Pair {
    pub query: String,
    pub skill: String,
    pub cwd: String,
}

fn user_text(msg: &Value) -> String {
    match &msg["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            if parts.iter().any(|p| p["type"] == "tool_result") {
                return String::new();
            }
            parts
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join(" ")
        }
        _ => String::new(),
    }
}

/// The last real user message before each Skill call is the query.
pub fn mine(paths: &Paths, days: u64) -> Vec<Pair> {
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(days * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut pairs = Vec::new();
    let mut seen = BTreeSet::new();
    let root = paths.user_home.join(".claude").join("projects");
    let files = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .flat_map(|d| fs::read_dir(d.path()).into_iter().flatten().filter_map(|e| e.ok()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t >= cutoff));
    for file in files {
        let Ok(f) = fs::File::open(&file) else { continue };
        let (mut last, mut cwd) = (None::<String>, String::new());
        for line in BufReader::new(f).lines().map_while(Result::ok) {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(c) = msg["cwd"].as_str() {
                cwd = c.to_string();
            }
            match msg["type"].as_str() {
                Some("user") => {
                    let text = user_text(&msg);
                    let text = text.trim();
                    if text.chars().count() > 4
                        && !["<", "# ", "[Earlier", "Base directory for this skill"].iter().any(|p| text.starts_with(p))
                    {
                        last = Some(text.chars().take(400).collect());
                    }
                }
                Some("assistant") => {
                    for part in msg["message"]["content"].as_array().into_iter().flatten() {
                        if part["type"] != "tool_use" || part["name"] != "Skill" {
                            continue;
                        }
                        let skill = part["input"]["skill"].as_str().unwrap_or("");
                        let skill = skill.rsplit(':').next().unwrap_or(skill).to_string();
                        if let (Some(query), false) = (last.take(), skill.is_empty())
                            && seen.insert((query.clone(), skill.clone()))
                        {
                            pairs.push(Pair { query, skill, cwd: cwd.clone() });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    pairs
}

#[derive(Default)]
struct Score {
    top1: usize,
    top5: usize,
}

/// Recall of the picked Skill among `find`'s Skills, with and without the
/// usage prior, over pairs whose Skill is still in the catalog.
pub fn run(paths: &Paths, finder: &Finder, catalog: &BTreeSet<String>, days: u64) -> Value {
    let pairs: Vec<Pair> = mine(paths, days).into_iter().filter(|p| catalog.contains(&p.skill)).collect();
    let (mut with, mut text) = (Score::default(), Score::default());
    let mut misses = Vec::new();
    for p in &pairs {
        let skills = |hits: Vec<Value>| -> Vec<String> {
            hits.into_iter()
                .filter(|h| h["kind"] == "skill")
                .filter_map(|h| h["name"].as_str().map(String::from))
                .collect()
        };
        let root = project_root(Path::new(&p.cwd));
        let ranked = skills(finder.hits(&p.query, Some(&root), 8));
        let plain = skills(finder.hits(&p.query, None, 8));
        for (names, s) in [(&ranked, &mut with), (&plain, &mut text)] {
            s.top1 += usize::from(names.first() == Some(&p.skill));
            s.top5 += usize::from(names.iter().take(5).any(|n| n == &p.skill));
        }
        if !ranked.iter().take(5).any(|n| n == &p.skill) {
            misses.push(json!({"skill": p.skill, "query": p.query.chars().take(80).collect::<String>(),
                "got": ranked.iter().take(3).collect::<Vec<_>>()}));
        }
    }
    let n = pairs.len().max(1) as f64;
    let pct = |x: usize| (x as f64 / n * 1000.0).round() / 10.0;
    json!({
        "pairs": pairs.len(),
        "skills": pairs.iter().map(|p| p.skill.as_str()).collect::<BTreeSet<_>>().len(),
        "days": days,
        "top1": pct(with.top1), "top5": pct(with.top5),
        "textTop1": pct(text.top1), "textTop5": pct(text.top5),
        "misses": misses,
    })
}
