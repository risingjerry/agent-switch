//! `mcp import`: adopt MCP servers already configured in native agent configs,
//! moving inline credentials into the secret store.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};

use crate::config::{
    Apps, Config, Mode, SECRET_REFERENCE_RE, SENSITIVE_ARG_FLAG_RE, SENSITIVE_NAME_RE, TOKEN_VALUE_RE, Tool,
    normalize_tool_id, put_tool, validate,
};
use crate::paths::Paths;
use crate::render::{entry_key, find_mcp_section, split_entries};
use crate::util::{Result, WriteResult, json_pretty, read_text, splitlines, write_if_changed};

pub struct Native {
    pub app: String,
    pub source_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub required_secrets: Vec<String>,
}

pub struct Skipped {
    pub app: String,
    pub id: String,
    pub reason: String,
}

#[derive(Default)]
pub struct Discovery {
    pub mcps: Vec<Native>,
    pub skipped: Vec<Skipped>,
}

pub struct Plan {
    pub config: Config,
    pub secrets: BTreeMap<String, String>,
    pub imported: Vec<String>,
    pub merged: Vec<String>,
}

impl Plan {
    pub fn public(&self) -> Value {
        json!({"imported": self.imported, "merged": self.merged, "secretNames": self.secrets.keys().collect::<Vec<_>>()})
    }
}

fn derived_secret_name(server_id: &str, index: usize) -> String {
    let mut slug = String::new();
    for c in server_id.to_uppercase().chars() {
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    format!("MCP_{}_SECRET_ARG_{}", if slug.is_empty() { "MCP" } else { slug }, index + 1)
}

type Sanitized = (Vec<String>, BTreeMap<String, String>, Vec<String>);

fn sanitize_args(server_id: &str, args: &[String]) -> Sanitized {
    let mut values = BTreeMap::new();
    let mut required = BTreeSet::new();
    let mut rendered = Vec::new();
    let mut previous = String::new();
    for (index, arg) in args.iter().enumerate() {
        let refs: Vec<String> = SECRET_REFERENCE_RE.captures_iter(arg).map(|c| c[1].to_string()).collect();
        required.extend(refs.iter().filter(|n| SENSITIVE_NAME_RE.is_match(n)).cloned());
        let assignment = SENSITIVE_ARG_FLAG_RE.is_match(arg);
        let after_flag = SENSITIVE_ARG_FLAG_RE.is_match(&previous) && !previous.contains('=');
        let header = (previous == "--header" || previous == "-H") && SENSITIVE_NAME_RE.is_match(arg);
        let literal = TOKEN_VALUE_RE.is_match(arg);
        let migrate = refs.is_empty() && (after_flag || header || literal);
        if assignment && arg.contains('=') && refs.is_empty() {
            let (prefix, secret) = arg.split_once('=').unwrap();
            if !secret.is_empty() {
                let name = derived_secret_name(server_id, index);
                values.insert(name.clone(), secret.to_string());
                rendered.push(format!("{prefix}=${{{name}}}"));
                required.insert(name);
                previous = arg.clone();
                continue;
            }
        }
        if migrate {
            let name = derived_secret_name(server_id, index);
            values.insert(name.clone(), arg.clone());
            rendered.push(format!("${{{name}}}"));
            required.insert(name);
        } else {
            rendered.push(arg.clone());
        }
        previous = arg.clone();
    }
    (rendered, values, required.into_iter().collect())
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.into()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn scan(app: &str, id: &str, value: &Value, out: &mut Discovery) -> Result<()> {
    let obj = value.as_object();
    let has_command = obj.and_then(|o| o.get("command")).is_some_and(|c| c.as_str().is_some_and(|s| !s.is_empty()));
    if let Some(o) = obj
        && !has_command
        && o.get("url").is_some_and(|u| !u.is_null() && u != "")
    {
        let raw = o.get("type").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("remote").to_lowercase();
        let transport = if ["http", "sse", "streamable-http"].contains(&raw.as_str()) { raw } else { "remote".into() };
        out.skipped.push(Skipped {
            app: app.into(),
            id: id.into(),
            reason: format!("{transport} transport requires the remote MCP adapter"),
        });
        return Ok(());
    }
    let o = obj.ok_or_else(|| format!("{app} MCP {id} must be an object"))?;
    let command = o
        .get("command")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{app} MCP {id} has no command"))?;
    let raw_args = match o.get("args") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(scalar_string)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("{app} MCP {id} args must be a list"))?,
        Some(_) => return Err(format!("{app} MCP {id} args must be a list")),
    };
    let mut env: BTreeMap<String, String> = match o.get("env") {
        None => BTreeMap::new(),
        Some(Value::Object(m)) => {
            m.iter().map(|(k, v)| (k.clone(), scalar_string(v).unwrap_or_else(|| v.to_string()))).collect()
        }
        Some(_) => return Err(format!("{app} MCP {id} env must be an object")),
    };
    let (args, arg_secrets, referenced) = sanitize_args(id, &raw_args);
    for (name, secret) in arg_secrets {
        if env.get(&name).is_some_and(|v| *v != secret) {
            return Err(format!("{app} MCP {id} has a derived argument secret conflict"));
        }
        env.insert(name, secret);
    }
    out.mcps.push(Native {
        app: app.into(),
        source_id: id.into(),
        command: command.into(),
        args,
        env,
        required_secrets: referenced,
    });
    Ok(())
}

fn scan_json(path: &Path, app: &str, out: &mut Discovery) -> Result<()> {
    let text = read_text(path)?;
    if !path.exists() {
        return Ok(());
    }
    let root: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid {app} JSON at {}: {e}", path.display()))?;
    let servers = match root.get("mcpServers") {
        None => return Ok(()),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => return Err(format!("{app} mcpServers must be an object")),
    };
    for (id, spec) in &servers {
        if !id.starts_with("agent-") {
            scan(app, id, spec, out)?;
        }
    }
    Ok(())
}

fn toml_to_json(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Datetime(d) => json!(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => Value::Object(t.iter().map(|(k, v)| (k.clone(), toml_to_json(v))).collect()),
    }
}

fn scan_codex(path: &Path, out: &mut Discovery) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let table: toml::Table = read_text(path)?
        .parse()
        .map_err(|e: toml::de::Error| format!("invalid codex TOML at {}: {}", path.display(), e.message()))?;
    let servers = match table.get("mcp_servers") {
        None => return Ok(()),
        Some(toml::Value::Table(t)) => t.clone(),
        Some(_) => return Err("codex mcp_servers must be a table".into()),
    };
    for (id, spec) in &servers {
        if !id.starts_with("agent-") {
            scan("codex", id, &toml_to_json(spec), out)?;
        }
    }
    Ok(())
}

fn yaml_scalar(raw: &str) -> Value {
    let v = raw.trim();
    if v == "[]" {
        return json!([]);
    }
    serde_json::from_str(v).unwrap_or_else(|_| json!(v.trim_matches(['\'', '"'])))
}

fn hermes_chunk(chunk: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    let mut i = 1;
    let key_line = |line: &str| -> Option<(String, String)> {
        let rest = line.strip_prefix("    ")?;
        let colon = rest.find(':')?;
        let key = &rest[..colon];
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        Some((key.to_string(), rest[colon + 1..].trim().to_string()))
    };
    while i < chunk.len() {
        let Some((key, raw)) = key_line(chunk[i]) else {
            i += 1;
            continue;
        };
        if !raw.is_empty() {
            result.insert(key, yaml_scalar(&raw));
            i += 1;
            continue;
        }
        i += 1;
        if key == "args" {
            let mut values = Vec::new();
            while let Some(item) = chunk.get(i).and_then(|l| l.strip_prefix("      -")) {
                let v = yaml_scalar(item);
                values.push(json!(scalar_string(&v).unwrap_or_else(|| v.to_string())));
                i += 1;
            }
            result.insert(key, Value::Array(values));
        } else if key == "env" {
            let mut values = serde_json::Map::new();
            while let Some(rest) = chunk.get(i).and_then(|l| l.strip_prefix("      ")) {
                let Some((name, val)) = rest.split_once(':') else { break };
                if !crate::config::ENV_NAME_RE.is_match(name) {
                    break;
                }
                let v = yaml_scalar(val);
                values.insert(name.into(), json!(scalar_string(&v).unwrap_or_else(|| v.to_string())));
                i += 1;
            }
            result.insert(key, Value::Object(values));
        }
    }
    Value::Object(result)
}

fn scan_hermes(path: &Path, out: &mut Discovery) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let text = read_text(path)?;
    let lines = splitlines(&text);
    let Some((start, end)) = find_mcp_section(&lines) else { return Ok(()) };
    for chunk in split_entries(&lines[start + 1..end]) {
        match entry_key(&chunk) {
            Some(id) if !id.starts_with("agent-") => scan("hermes", &id, &hermes_chunk(&chunk), out)?,
            _ => {}
        }
    }
    Ok(())
}

pub fn discover(paths: &Paths, apps: &[String]) -> Result<Discovery> {
    let mut out = Discovery::default();
    let has = |a: &str| apps.iter().any(|x| x == a);
    if has("claude") {
        scan_json(&paths.claude_config, "claude", &mut out)?;
    }
    if has("claude_desktop") {
        scan_json(&paths.claude_desktop_config, "claude_desktop", &mut out)?;
    }
    if has("codex") {
        scan_codex(&paths.codex_config, &mut out)?;
    }
    if has("hermes") {
        scan_hermes(&paths.hermes_config, &mut out)?;
    }
    Ok(out)
}

pub fn plan(config: &Config, natives: &[Native]) -> Result<Plan> {
    let mut updated = config.clone();
    let mut secrets = BTreeMap::new();
    let mut imported = BTreeSet::new();
    let mut merged = BTreeSet::new();
    for native in natives {
        let id = normalize_tool_id(&native.source_id)?;
        let (private, public): (BTreeMap<_, _>, BTreeMap<_, _>) = native
            .env
            .clone()
            .into_iter()
            .partition(|(k, v)| SENSITIVE_NAME_RE.is_match(k) || TOKEN_VALUE_RE.is_match(v));
        for (name, value) in &private {
            if secrets.get(name).is_some_and(|v| v != value) {
                return Err(format!("conflicting values found for secret name {name}"));
            }
            secrets.insert(name.clone(), value.clone());
        }
        let mut apps = Apps::none();
        apps.set(&native.app, true);
        let required: BTreeSet<String> =
            private.keys().cloned().chain(native.required_secrets.iter().cloned()).collect();
        let candidate = Tool {
            id: id.clone(),
            name: native.source_id.clone(),
            command: native.command.clone(),
            args: native.args.clone(),
            required_secrets: required.into_iter().collect(),
            apps,
            env: public,
            description: json!(format!("Imported from {}", native.app)),
            enabled: true,
            mode: Mode::Direct,
            idle_seconds: None,
        };
        let Some(existing) = updated.find(&id).cloned() else {
            updated = put_tool(updated, candidate, true)?;
            imported.insert(id);
            continue;
        };
        let env_conflict = candidate.env.iter().any(|(k, v)| existing.env.get(k).is_some_and(|e| e != v));
        if existing.command != candidate.command || existing.args != candidate.args || env_conflict {
            return Err(format!("MCP import conflict for {id}; commands or environments differ between sources"));
        }
        let mut next = existing.clone();
        next.apps.set(&native.app, true);
        next.env.extend(candidate.env);
        let mut req: BTreeSet<String> = next.required_secrets.iter().cloned().collect();
        req.extend(candidate.required_secrets);
        next.required_secrets = req.into_iter().collect();
        updated = put_tool(updated, next, false)?;
        merged.insert(id);
    }
    validate(&updated)?;
    Ok(Plan {
        config: updated,
        secrets,
        imported: imported.into_iter().collect(),
        merged: merged.into_iter().collect(),
    })
}

// ---- --adopt: remove imported entries from their source configs ----

fn remove_json(path: &Path, ids: &BTreeSet<String>, backup: &Path) -> Result<Option<WriteResult>> {
    if !path.exists() || ids.is_empty() {
        return Ok(None);
    }
    let mut root: Value = serde_json::from_str(&read_text(path)?).map_err(|e| e.to_string())?;
    let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else { return Ok(None) };
    let mut changed = false;
    for id in ids {
        changed |= servers.remove(id).is_some();
    }
    if !changed {
        return Ok(None);
    }
    write_if_changed(path, &(json_pretty(&root) + "\n"), None, Some(backup)).map(Some)
}

fn remove_codex(path: &Path, ids: &BTreeSet<String>, backup: &Path) -> Result<Option<WriteResult>> {
    if !path.exists() || ids.is_empty() {
        return Ok(None);
    }
    let original = read_text(path)?;
    let mut rendered = original.clone();
    for id in ids {
        let headers = [format!("[mcp_servers.{id}]"), format!("[mcp_servers.\"{id}\"]")];
        let mut out = String::new();
        let mut skipping = false;
        let mut rest = rendered.as_str();
        while !rest.is_empty() {
            let (line, next) = match rest.find('\n') {
                Some(i) => (&rest[..=i], &rest[i + 1..]),
                None => (rest, ""),
            };
            if skipping && line.starts_with('[') {
                skipping = false;
            }
            let bare = line.trim_end();
            if !skipping && line.ends_with('\n') && headers.iter().any(|h| bare == h) {
                skipping = true;
            }
            if !skipping {
                out.push_str(line);
            }
            rest = next;
        }
        let mut kept: Vec<&str> = Vec::new();
        let mut in_mcp = false;
        for line in splitlines(&out) {
            let s = line.trim();
            if s.starts_with('[') && s.ends_with(']') {
                in_mcp = s == "[mcp_servers]";
            }
            let inline = [id.clone(), format!("\"{id}\"")]
                .iter()
                .any(|k| s.strip_prefix(k.as_str()).is_some_and(|r| r.trim_start().starts_with('=')));
            if in_mcp && inline {
                continue;
            }
            kept.push(line);
        }
        let joined = kept.join("\n");
        rendered = format!("{}{}", joined.trim_end(), if kept.is_empty() { "" } else { "\n" });
    }
    if rendered == original {
        return Ok(None);
    }
    rendered.parse::<toml::Table>().map_err(|e| {
        format!("refusing to adopt Codex MCP because cleanup would produce invalid TOML: {}", e.message())
    })?;
    write_if_changed(path, &rendered, None, Some(backup)).map(Some)
}

fn remove_hermes(path: &Path, ids: &BTreeSet<String>, backup: &Path) -> Result<Option<WriteResult>> {
    if !path.exists() || ids.is_empty() {
        return Ok(None);
    }
    let original = read_text(path)?;
    let lines = splitlines(&original);
    let Some((start, end)) = find_mcp_section(&lines) else { return Ok(None) };
    let mut out: Vec<&str> = lines[..=start].to_vec();
    for chunk in split_entries(&lines[start + 1..end]) {
        if !entry_key(&chunk).is_some_and(|k| ids.contains(&k)) {
            out.extend(chunk);
        }
    }
    out.extend(&lines[end..]);
    let rendered = format!("{}\n", out.join("\n").trim_end());
    if rendered == original {
        return Ok(None);
    }
    write_if_changed(path, &rendered, None, Some(backup)).map(Some)
}

pub fn adopt(paths: &Paths, natives: &[Native]) -> Result<usize> {
    let mut by_app: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for n in natives {
        by_app.entry(n.app.as_str()).or_default().insert(n.source_id.clone());
    }
    let empty = BTreeSet::new();
    let ids = |a: &str| by_app.get(a).unwrap_or(&empty);
    let b = &paths.backup_dir;
    let results = [
        remove_json(&paths.claude_config, ids("claude"), b)?,
        remove_json(&paths.claude_desktop_config, ids("claude_desktop"), b)?,
        remove_codex(&paths.codex_config, ids("codex"), b)?,
        remove_hermes(&paths.hermes_config, ids("hermes"), b)?,
    ];
    Ok(results.iter().filter(|r| r.is_some()).count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_inline_credentials() {
        let args: Vec<String> = ["--api-key=abc", "--token", "xyz", "plain", "${MY_TOKEN}"].map(String::from).to_vec();
        let (rendered, values, required) = sanitize_args("my srv", &args);
        assert_eq!(
            rendered,
            vec![
                "--api-key=${MCP_MY_SRV_SECRET_ARG_1}",
                "--token",
                "${MCP_MY_SRV_SECRET_ARG_3}",
                "plain",
                "${MY_TOKEN}"
            ]
        );
        assert_eq!(values["MCP_MY_SRV_SECRET_ARG_1"], "abc");
        assert_eq!(values["MCP_MY_SRV_SECRET_ARG_3"], "xyz");
        assert_eq!(required, vec!["MCP_MY_SRV_SECRET_ARG_1", "MCP_MY_SRV_SECRET_ARG_3", "MY_TOKEN"]);
    }
}
