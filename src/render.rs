//! Native config renderers. Each one rewrites only `agent-*` MCP entries and
//! preserves everything else byte-for-byte where the format allows.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value};

use crate::util::{Result, json_pretty, json_str_ascii, splitlines};

pub const AGENT_PREFIX: &str = "agent-";

/// Desired MCP projection: server id → {command, args}.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub command: String,
    pub args: Vec<String>,
}

pub type Desired = BTreeMap<String, Spec>;

impl Spec {
    pub fn to_value(&self) -> Value {
        serde_json::json!({"command": self.command, "args": self.args})
    }
}

// ---- Claude Code / Claude Desktop (JSON) ----

/// True when the `agent-*` servers in a JSON config already equal `desired`.
/// Claude Code rewrites `~/.claude.json` constantly with its own key order and
/// number formatting, so a byte comparison would report drift forever and
/// make reconcile rewrite a file another process owns.
pub fn json_in_sync(current: &str, desired: &Desired) -> bool {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(current) else { return false };
    let empty = Map::new();
    let servers = match root.get("mcpServers") {
        Some(Value::Object(m)) => m,
        None | Some(Value::Null) => &empty,
        Some(_) => return false,
    };
    let ours: Vec<(&String, &Value)> = servers.iter().filter(|(k, _)| k.starts_with(AGENT_PREFIX)).collect();
    ours.len() == desired.len() && ours.iter().all(|(k, v)| desired.get(*k).is_some_and(|s| **v == s.to_value()))
}

pub fn json_config(current: &str, desired: &Desired) -> Result<String> {
    let mut root = if current.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(current) {
            Ok(Value::Object(m)) => m,
            Ok(_) => return Err("JSON config root must be an object".into()),
            Err(e) => return Err(format!("malformed JSON config: {e}")),
        }
    };
    let servers = match root.remove("mcpServers") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(m)) => m,
        Some(_) => return Err("mcpServers must be an object".into()),
    };
    let mut merged: Map<String, Value> = servers.into_iter().filter(|(k, _)| !k.starts_with(AGENT_PREFIX)).collect();
    for (id, spec) in desired {
        merged.insert(id.clone(), spec.to_value());
    }
    root.insert("mcpServers".into(), Value::Object(merged));
    Ok(json_pretty(&Value::Object(root)) + "\n")
}

// ---- Codex (TOML, text-preserving) ----

fn toml_scalar(value: &str) -> String {
    json_str_ascii(value)
}

/// `[mcp_servers.agent-x]` / `[mcp_servers."agent-x"]` / nested
/// `[mcp_servers.agent-x.env]` headers with nothing but whitespace after `]`.
fn is_agent_subtable_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("[mcp_servers.") else { return false };
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let Some(rest) = rest.strip_prefix(AGENT_PREFIX) else { return false };
    let end = rest.find([']', '"']).unwrap_or(rest.len());
    if end == 0 {
        return false;
    }
    let rest = &rest[end..];
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    match rest.strip_prefix(']') {
        Some(tail) => tail.chars().all(|c| c.is_whitespace() && c != '\n'),
        None => false,
    }
}

/// Port of the Python regex `(?ms)^\[mcp_servers\."?agent-[^\]"]+"?\]\s*\n.*?(?=^\[|\Z)`.
fn strip_agent_subtables(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skipping = false;
    let mut rest = text;
    while !rest.is_empty() {
        let (line, next) = match rest.find('\n') {
            Some(i) => (&rest[..=i], &rest[i + 1..]),
            None => (rest, ""),
        };
        if skipping && line.starts_with('[') {
            skipping = false;
        }
        if !skipping
            && line.ends_with('\n')
            && is_agent_subtable_header(line.trim_end_matches('\n').trim_end_matches('\r'))
        {
            skipping = true;
        }
        if !skipping {
            out.push_str(line);
        }
        rest = next;
    }
    out
}

fn strip_agent_inline_entries(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_mcp = false;
    for line in splitlines(text) {
        let s = line.trim();
        if s.starts_with('[') && s.ends_with(']') {
            in_mcp = s == "[mcp_servers]";
        }
        if in_mcp && s.starts_with(AGENT_PREFIX) && s.contains('=') {
            continue;
        }
        out.push(line);
    }
    let joined = out.join("\n");
    format!("{}{}", joined.trim_end(), if out.is_empty() { "" } else { "\n" })
}

fn with_model_instructions_file(text: &str, file: Option<&Path>) -> String {
    let Some(file) = file else { return text.to_string() };
    let mut lines: Vec<String> = splitlines(text).into_iter().map(String::from).collect();
    let rendered = format!("model_instructions_file = {}", toml_scalar(&file.to_string_lossy()));
    let table_idx = lines.iter().position(|l| l.trim().starts_with('[')).unwrap_or(lines.len());
    let is_setting = |l: &str| {
        let s = l.trim();
        s.strip_prefix("model_instructions_file").is_some_and(|r| r.trim_start().starts_with('='))
    };
    if let Some(idx) = (0..table_idx).find(|&i| is_setting(&lines[i])) {
        lines[idx] = rendered;
    } else {
        lines.insert(table_idx, rendered);
    }
    format!("{}\n", lines.join("\n").trim_end())
}

pub fn codex_config(current: &str, desired: &Desired, instruction_file: Option<&Path>) -> Result<String> {
    if !current.trim().is_empty() {
        current.parse::<toml::Table>().map_err(|e| format!("malformed TOML config: {}", e.message()))?;
    }
    let text = strip_agent_subtables(current);
    let text = strip_agent_inline_entries(&text);
    let text = with_model_instructions_file(&text, instruction_file);
    let mut chunks: Vec<String> = Vec::new();
    if !text.trim().is_empty() {
        chunks.push(text.trim_end().to_string());
    }
    for (id, spec) in desired {
        let args: Vec<String> = spec.args.iter().map(|a| toml_scalar(a)).collect();
        chunks.push(format!(
            "[mcp_servers.{id}]\nargs = [{}]\ncommand = {}",
            args.join(", "),
            toml_scalar(&spec.command)
        ));
    }
    let joined = chunks.join("\n\n");
    Ok(format!("{}\n", joined.trim_end()))
}

// ---- DeepSeek Harness (cordis patch list, line-preserving) ----

const DSH_START: &str = "# BEGIN FUSEBOX MANAGED MCP — edits inside this block are overwritten";
const DSH_END: &str = "# END FUSEBOX MANAGED MCP";

/// `~/.dsh/cordis.patch.yml` is a top-level YAML list applied to every dsh
/// profile. Fusebox owns one marked block of `insert` rows mounting
/// `dsh-mcp-client`; everything outside the block is kept verbatim.
pub fn dsh_patch(current: &str, desired: &Desired) -> Result<String> {
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in splitlines(current) {
        if line.starts_with("# BEGIN FUSEBOX MANAGED MCP") {
            inside = true;
            continue;
        }
        if inside {
            inside = line != DSH_END;
            continue;
        }
        kept.push(line);
    }
    let content: Vec<&str> = kept.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let empty_flow = content == ["[]"];
    if !empty_flow && content.first().is_some_and(|l| l.starts_with('[') || l.starts_with('{')) {
        return Err("dsh patch uses YAML flow style; convert it to a block list (`- id: ...`) first".into());
    }
    if empty_flow {
        kept.retain(|l| l.trim() != "[]");
    }
    let base = kept.join("\n");
    let base = base.trim_end();
    let mut out = String::from(base);
    if desired.is_empty() {
        if content.is_empty() || empty_flow {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[]");
        }
        out.push('\n');
        return Ok(out);
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(DSH_START);
    out.push_str("\n- insert:\n");
    for (id, spec) in desired {
        let args: Vec<String> = spec.args.iter().map(|a| json_str_ascii(a)).collect();
        out.push_str(&format!(
            "    - id: {id}\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: {id}\n        transport: stdio\n        command: {}\n        args: [{}]\n",
            json_str_ascii(&spec.command),
            args.join(", ")
        ));
    }
    out.push_str(DSH_END);
    out.push('\n');
    Ok(out)
}

// ---- Hermes (YAML subset, line-preserving) ----

fn is_top_level_key(line: &str) -> bool {
    let key_len = line.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '\\' | '-')).count();
    key_len > 0 && line[key_len..].starts_with(':')
}

pub fn find_mcp_section(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.trim() == "mcp_servers:" && !l.starts_with([' ', '\t']))?;
    let end = (start + 1..lines.len())
        .find(|&i| {
            let l = lines[i];
            !l.is_empty() && !l.starts_with([' ', '\t']) && is_top_level_key(l)
        })
        .unwrap_or(lines.len());
    Some((start, end))
}

/// Matches `^  [^:\s][^:]*:\s*$`.
fn is_entry_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("  ") else { return false };
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if c != ':' && !c.is_whitespace() => {}
        _ => return false,
    }
    let Some(colon) = rest.find(':') else { return false };
    rest[colon + 1..].chars().all(char::is_whitespace)
}

pub fn split_entries<'a>(section: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut chunks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for &line in section {
        if is_entry_header(line) {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            current.push(line);
        } else if !current.is_empty() {
            current.push(line);
        } else {
            chunks.push(vec![line]);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn entry_key(chunk: &[&str]) -> Option<String> {
    let first = chunk.first()?;
    if !is_entry_header(first) {
        return None;
    }
    let key = first.trim().split(':').next().unwrap_or("");
    Some(key.trim_matches(['\'', '"']).to_string())
}

fn render_agent_yaml(desired: &Desired) -> Vec<String> {
    let mut lines = Vec::new();
    for (id, spec) in desired {
        lines.push(format!("  {id}:"));
        if spec.args.is_empty() {
            lines.push("    args: []".into());
        } else {
            lines.push("    args:".into());
            lines.extend(spec.args.iter().map(|a| format!("      - {}", json_str_ascii(a))));
        }
        lines.push(format!("    command: {}", json_str_ascii(&spec.command)));
    }
    lines
}

pub fn hermes_config(current: &str, desired: &Desired) -> String {
    let lines = splitlines(current);
    let desired_lines = render_agent_yaml(desired);
    let Some((start, end)) = find_mcp_section(&lines) else {
        let mut base: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        if base.last().is_some_and(|l| !l.trim().is_empty()) {
            base.push(String::new());
        }
        base.push("mcp_servers:".into());
        base.extend(desired_lines);
        return format!("{}\n", base.join("\n").trim_end());
    };
    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    out.push("mcp_servers:".into());
    for chunk in split_entries(&lines[start + 1..end]) {
        match entry_key(&chunk) {
            Some(key) if key.starts_with(AGENT_PREFIX) => {}
            _ => out.extend(chunk.iter().map(|s| s.to_string())),
        }
    }
    out.extend(desired_lines);
    out.extend(lines[end..].iter().map(|s| s.to_string()));
    format!("{}\n", out.join("\n").trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desired() -> Desired {
        let mut d = Desired::new();
        d.insert("agent-b".into(), Spec { command: "/bin/mcp-b".into(), args: vec![] });
        d.insert("agent-a".into(), Spec { command: "/bin/mcp-a".into(), args: vec!["x".into(), "é".into()] });
        d
    }

    #[test]
    fn json_keeps_foreign_servers_and_sorts() {
        let current =
            r#"{"z": 1, "mcpServers": {"agent-old": {"command": "o"}, "mine": {"command": "m"}}, "a": [1.5]}"#;
        let out = json_config(current, &desired()).unwrap();
        let expected = "{\n  \"a\": [\n    1.5\n  ],\n  \"mcpServers\": {\n    \"agent-a\": {\n      \"args\": [\n        \"x\",\n        \"é\"\n      ],\n      \"command\": \"/bin/mcp-a\"\n    },\n    \"agent-b\": {\n      \"args\": [],\n      \"command\": \"/bin/mcp-b\"\n    },\n    \"mine\": {\n      \"command\": \"m\"\n    }\n  },\n  \"z\": 1\n}\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn json_sync_ignores_foreign_formatting() {
        let d = desired();
        let rendered = json_config(r#"{"b": 1, "a": 2.50}"#, &d).unwrap();
        let reformatted = rendered.replace("\n", "").replace("  ", "");
        assert!(json_in_sync(&reformatted, &d));
        let mut changed = d.clone();
        changed.get_mut("agent-a").unwrap().args.push("y".into());
        assert!(!json_in_sync(&rendered, &changed));
        assert!(!json_in_sync(r#"{"mcpServers": {"agent-x": {}}}"#, &Desired::new()));
        assert!(json_in_sync(r#"{"mcpServers": {"mine": {}}}"#, &Desired::new()));
    }

    #[test]
    fn codex_replaces_agent_tables_and_sets_instructions() {
        let current = "model = \"x\"\n\n[mcp_servers.agent-old]\ncommand = \"o\"\nargs = []\n\n[mcp_servers.agent-old.env]\nA = \"1\"\n\n[mcp_servers.keep]\ncommand = \"k\"\n";
        let out = codex_config(current, &desired(), Some(Path::new("/i/AGENTS.md"))).unwrap();
        let expected = "model = \"x\"\n\nmodel_instructions_file = \"/i/AGENTS.md\"\n[mcp_servers.keep]\ncommand = \"k\"\n\n[mcp_servers.agent-a]\nargs = [\"x\", \"\\u00e9\"]\ncommand = \"/bin/mcp-a\"\n\n[mcp_servers.agent-b]\nargs = []\ncommand = \"/bin/mcp-b\"\n";
        assert_eq!(out, expected);
        assert!(codex_config("not = = toml", &desired(), None).is_err());
    }

    #[test]
    fn dsh_patch_owns_one_block() {
        let current = "# header\n- id: session-log-deepseek\n  config:\n    enabled: false\n";
        let out = dsh_patch(current, &desired()).unwrap();
        assert!(out.starts_with(current.trim_end()));
        assert!(out.contains("- insert:\n    - id: agent-a\n      name: '@deepseek-ai/dsh-mcp-client'"));
        assert!(out.contains("        args: [\"x\", \"\\u00e9\"]\n"));
        // Re-rendering is stable and removing everything restores the original.
        assert_eq!(dsh_patch(&out, &desired()).unwrap(), out);
        assert_eq!(dsh_patch(&out, &Desired::new()).unwrap(), current);
        // An empty flow list becomes a block list, and back.
        let from_empty = dsh_patch("# c\n[]\n", &desired()).unwrap();
        assert!(!from_empty.contains("[]\n# BEGIN"));
        assert_eq!(dsh_patch(&from_empty, &Desired::new()).unwrap(), "# c\n[]\n");
        assert!(dsh_patch("[{id: x}]", &desired()).is_err());
    }

    #[test]
    fn hermes_rewrites_section() {
        let current =
            "model: x\nmcp_servers:\n  keep:\n    command: \"k\"\n  agent-old:\n    command: \"o\"\nother: 1\n";
        let out = hermes_config(current, &desired());
        let expected = "model: x\nmcp_servers:\n  keep:\n    command: \"k\"\n  agent-a:\n    args:\n      - \"x\"\n      - \"\\u00e9\"\n    command: \"/bin/mcp-a\"\n  agent-b:\n    args: []\n    command: \"/bin/mcp-b\"\nother: 1\n";
        assert_eq!(out, expected);
        assert_eq!(hermes_config("", &Desired::new()), "mcp_servers:\n");
    }
}
