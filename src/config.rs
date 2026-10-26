//! Central config model (`config.json`), validation, loading and locked updates.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex_lite::Regex;
use serde_json::{Map, Value, json};

use crate::util::{Result, WriteResult, expand_user, json_pretty, write_if_changed};

pub const APP_NAMES: [&str; 4] = ["claude", "claude_desktop", "codex", "hermes"];

pub static ENV_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());
pub static SECRET_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9_]*$").unwrap());
pub static SENSITIVE_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:KEY|TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL|AUTH)").unwrap());
pub static SECRET_REFERENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{([A-Z][A-Z0-9_]*)\}").unwrap());
pub static SENSITIVE_ARG_FLAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^--?(?:api[-_]?key|token|secret|password|passwd|credential|authorization|auth-token)(?:=|$)")
        .unwrap()
});
pub static TOKEN_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(sk-[A-Za-z0-9_\-]{8,}|xai-[A-Za-z0-9_\-]{8,}|tvly-[A-Za-z0-9_\-]{8,}|AIza[A-Za-z0-9_\-]{12,}|[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,})",
    )
    .unwrap()
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Apps {
    pub claude: bool,
    pub claude_desktop: bool,
    pub codex: bool,
    pub hermes: bool,
}

impl Default for Apps {
    fn default() -> Self {
        Apps { claude: true, claude_desktop: true, codex: true, hermes: true }
    }
}

impl Apps {
    pub fn none() -> Apps {
        Apps { claude: false, claude_desktop: false, codex: false, hermes: false }
    }
    pub fn get(&self, app: &str) -> bool {
        match app {
            "claude" => self.claude,
            "claude_desktop" => self.claude_desktop,
            "codex" => self.codex,
            "hermes" => self.hermes,
            _ => false,
        }
    }
    pub fn set(&mut self, app: &str, on: bool) {
        match app {
            "claude" => self.claude = on,
            "claude_desktop" => self.claude_desktop = on,
            "codex" => self.codex = on,
            "hermes" => self.hermes = on,
            _ => {}
        }
    }
    fn from_value(value: Option<&Value>) -> Apps {
        let mut apps = Apps::default();
        if let Some(Value::Object(map)) = value
            && !map.is_empty()
        {
            for name in APP_NAMES {
                if let Some(v) = map.get(name) {
                    apps.set(name, truthy(v));
                }
            }
        }
        apps
    }
    pub fn to_value(self) -> Value {
        json!({"claude": self.claude, "claude_desktop": self.claude_desktop, "codex": self.codex, "hermes": self.hermes})
    }
    pub fn enabled_names(&self) -> Vec<&'static str> {
        APP_NAMES.iter().copied().filter(|n| self.get(n)).collect()
    }
}

/// How a tool is projected into agents: its own native entry, or served
/// lazily through the shared `agent-hub` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Direct,
    Hub,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub required_secrets: Vec<String>,
    pub apps: Apps,
    pub env: BTreeMap<String, String>,
    pub description: Value,
    pub enabled: bool,
    pub mode: Mode,
    pub idle_seconds: Option<u64>,
}

impl Tool {
    pub fn wrapper_name(&self) -> String {
        format!("mcp-{}", self.id.strip_prefix("agent-").unwrap_or(&self.id))
    }

    fn from_value(value: &Value) -> Result<Tool> {
        let obj = value.as_object().ok_or("tool entries must be objects")?;
        let id = obj.get("id").map(value_str).ok_or("tool id is required")?;
        let name = match obj.get("name").map(value_str) {
            Some(n) if !n.is_empty() => n,
            _ => id.clone(),
        };
        let command = obj.get("command").map(value_str).ok_or_else(|| format!("tool command is required: {id}"))?;
        let list = |key: &str| -> Vec<String> {
            obj.get(key).and_then(Value::as_array).map(|a| a.iter().map(value_str).collect()).unwrap_or_default()
        };
        let required =
            if obj.contains_key("requiredSecrets") { list("requiredSecrets") } else { list("required_secrets") };
        let env = obj
            .get("env")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), value_str(v))).collect())
            .unwrap_or_default();
        Ok(Tool {
            name,
            command,
            args: list("args"),
            required_secrets: required,
            apps: Apps::from_value(obj.get("apps")),
            env,
            description: obj.get("description").cloned().unwrap_or(Value::Null),
            enabled: obj.get("enabled").map(truthy).unwrap_or(true),
            mode: if obj.get("mode").and_then(Value::as_str) == Some("hub") { Mode::Hub } else { Mode::Direct },
            idle_seconds: obj.get("idleSeconds").and_then(Value::as_u64),
            id,
        })
    }

    pub fn to_public(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".into(), json!(self.id));
        map.insert("name".into(), json!(self.name));
        map.insert("command".into(), json!(self.command));
        map.insert("args".into(), json!(self.args));
        map.insert("requiredSecrets".into(), json!(self.required_secrets));
        map.insert("apps".into(), self.apps.to_value());
        map.insert("envNames".into(), json!(self.env.keys().collect::<Vec<_>>()));
        map.insert("description".into(), self.description.clone());
        map.insert("enabled".into(), json!(self.enabled));
        self.extend_optional(&mut map);
        Value::Object(map)
    }

    fn to_config(&self) -> Value {
        let mut value = self.to_public();
        let map = value.as_object_mut().unwrap();
        map.remove("envNames");
        if self.mode == Mode::Direct {
            map.remove("mode");
        }
        map.insert("env".into(), json!(self.env));
        value
    }

    /// Optional fields are only written when set, so configs that never use
    /// them stay byte-identical with the Python implementation.
    fn extend_optional(&self, map: &mut Map<String, Value>) {
        map.insert("mode".into(), json!(if self.mode == Mode::Hub { "hub" } else { "direct" }));
        if let Some(idle) = self.idle_seconds {
            map.insert("idleSeconds".into(), json!(idle));
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Routes {
    pub search_default: String,
    pub x_read_default: String,
    pub x_read_fallback: String,
}

impl Routes {
    fn from_value(value: Option<&Value>) -> Routes {
        let Some(Value::Object(m)) = value else { return Routes::default() };
        let pick = |a: &str, b: &str| m.get(a).or_else(|| m.get(b)).map(value_str).unwrap_or_default();
        Routes {
            search_default: pick("searchDefault", "search_default"),
            x_read_default: pick("xReadDefault", "x_read_default"),
            x_read_fallback: pick("xReadFallback", "x_read_fallback"),
        }
    }
    fn to_value(&self) -> Value {
        json!({"searchDefault": self.search_default, "xReadDefault": self.x_read_default, "xReadFallback": self.x_read_fallback})
    }
    pub fn clear(&mut self, id: &str) {
        for slot in [&mut self.search_default, &mut self.x_read_default, &mut self.x_read_fallback] {
            if slot == id {
                slot.clear();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub tools: Vec<Tool>,
    pub routes: Routes,
    pub secret_file: PathBuf,
    pub skill_hub: Option<PathBuf>,
}

impl Config {
    pub fn default_for(secret_file: &Path) -> Config {
        Config { tools: Vec::new(), routes: Routes::default(), secret_file: secret_file.to_path_buf(), skill_hub: None }
    }

    pub fn tools_for_app(&self, app: &str) -> Vec<&Tool> {
        self.tools.iter().filter(|t| t.enabled && t.apps.get(app)).collect()
    }

    pub fn find(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.id == id)
    }

    pub fn render(&self) -> String {
        let mut value = json!({
            "tools": self.tools.iter().map(Tool::to_config).collect::<Vec<_>>(),
            "routes": self.routes.to_value(),
            "secretFile": self.secret_file.to_string_lossy(),
        });
        if let Some(hub) = &self.skill_hub {
            value["skillHub"] = json!(hub.to_string_lossy());
        }
        json_pretty(&value) + "\n"
    }
}

fn value_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        other => other.to_string(),
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn load(config_file: &Path, default_secret_file: &Path) -> Result<Config> {
    let text = match fs::read_to_string(config_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default_for(default_secret_file)),
        Err(e) => return Err(format!("failed to read {}: {e}", config_file.display())),
    };
    let raw: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid config JSON at {}: {e}", config_file.display()))?;
    let obj = raw
        .as_object()
        .ok_or_else(|| format!("invalid config JSON at {}: root must be an object", config_file.display()))?;
    let secret_value = obj
        .get("secretFile")
        .or_else(|| obj.get("secret_file"))
        .map(value_str)
        .unwrap_or_else(|| default_secret_file.to_string_lossy().into_owned());
    let mut secret_file = expand_user(&secret_value);
    if !secret_file.is_absolute() {
        let base = config_file.parent().unwrap_or(Path::new("."));
        secret_file = crate::util::resolve_lenient(&base.join(secret_file));
    }
    let tools = obj
        .get("tools")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(Tool::from_value).collect::<Result<Vec<_>>>())
        .transpose()?
        .unwrap_or_default();
    let config = Config {
        tools,
        routes: Routes::from_value(obj.get("routes")),
        secret_file,
        skill_hub: obj.get("skillHub").and_then(Value::as_str).map(expand_user),
    };
    validate(&config)?;
    Ok(config)
}

pub fn validate(config: &Config) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for tool in &config.tools {
        let id = &tool.id;
        if !id.starts_with("agent-") {
            return Err(format!("managed tool id must start with agent-: {id}"));
        }
        if !seen.insert(id.as_str()) {
            return Err(format!("duplicate tool id: {id}"));
        }
        if tool.command.is_empty() {
            return Err(format!("tool command is required: {id}"));
        }
        if TOKEN_VALUE_RE.is_match(&tool.command) {
            return Err(format!("credential-shaped value must not be embedded in MCP command: {id}"));
        }
        let bad = sorted_filter(tool.required_secrets.iter(), |n| !SECRET_NAME_RE.is_match(n));
        if !bad.is_empty() {
            return Err(format!("invalid required secret name(s) for {id}: {}", bad.join(", ")));
        }
        let bad = sorted_filter(tool.env.keys(), |n| !ENV_NAME_RE.is_match(n));
        if !bad.is_empty() {
            return Err(format!("invalid MCP environment name(s) for {id}: {}", bad.join(", ")));
        }
        let bad = sorted_filter(tool.env.keys(), |n| SENSITIVE_NAME_RE.is_match(n));
        if !bad.is_empty() {
            return Err(format!(
                "sensitive MCP environment name(s) must use requiredSecrets for {id}: {}",
                bad.join(", ")
            ));
        }
        let bad = sorted_filter(tool.env.iter().filter(|(_, v)| TOKEN_VALUE_RE.is_match(v)).map(|(k, _)| k), |_| true);
        if !bad.is_empty() {
            return Err(format!(
                "credential-shaped MCP environment value(s) must use requiredSecrets for {id}: {}",
                bad.join(", ")
            ));
        }
        let mut previous = "";
        for (index, arg) in tool.args.iter().enumerate() {
            let refs: Vec<&str> = SECRET_REFERENCE_RE.captures_iter(arg).map(|c| c.get(1).unwrap().as_str()).collect();
            let undeclared = sorted_filter(refs.iter(), |n| {
                SENSITIVE_NAME_RE.is_match(n) && !tool.required_secrets.iter().any(|s| s == *n)
            });
            if !undeclared.is_empty() {
                return Err(format!(
                    "MCP argument references undeclared secret name(s) for {id}: {}",
                    undeclared.join(", ")
                ));
            }
            let assignment = SENSITIVE_ARG_FLAG_RE.is_match(arg) && arg.contains('=') && refs.is_empty();
            let follows = SENSITIVE_ARG_FLAG_RE.is_match(previous) && !previous.contains('=') && refs.is_empty();
            if TOKEN_VALUE_RE.is_match(arg) || assignment || follows {
                return Err(format!(
                    "credential-shaped MCP argument {} must use a declared ${{SECRET_NAME}} placeholder: {id}",
                    index + 1
                ));
            }
            previous = arg;
        }
        let overlap = sorted_filter(tool.required_secrets.iter(), |n| tool.env.contains_key(n.as_str()));
        if !overlap.is_empty() {
            return Err(format!("MCP env must not override required secret name(s): {}", overlap.join(", ")));
        }
    }
    let enabled: std::collections::BTreeSet<&str> =
        config.tools.iter().filter(|t| t.enabled).map(|t| t.id.as_str()).collect();
    let r = &config.routes;
    let mut missing: Vec<&str> = [&r.search_default, &r.x_read_default, &r.x_read_fallback]
        .into_iter()
        .filter(|v| !v.is_empty() && !enabled.contains(v.as_str()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        missing.sort();
        return Err(format!("route references unknown or disabled tool id(s): {}", missing.join(", ")));
    }
    Ok(())
}

fn sorted_filter<'a, I, S>(items: I, keep: impl Fn(&S) -> bool) -> Vec<String>
where
    I: Iterator<Item = &'a S>,
    S: AsRef<str> + 'a + ?Sized,
{
    let mut out: Vec<String> = items.filter(|s| keep(s)).map(|s| s.as_ref().to_string()).collect();
    out.sort();
    out.dedup();
    out
}

/// Exclusive advisory lock on `.<name>.lock` next to `target`, held until drop.
pub struct FileLock(File);

impl FileLock {
    pub fn acquire(target: &Path, exclusive: bool) -> Result<FileLock> {
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let lock_path = target.with_file_name(format!(".{name}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .map_err(|e| format!("failed to open lock {}: {e}", lock_path.display()))?;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        if exclusive { file.lock() } else { file.lock_shared() }
            .map_err(|e| format!("failed to lock {}: {e}", lock_path.display()))?;
        Ok(FileLock(file))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn update(
    config_path: &Path,
    default_secret_file: &Path,
    backup_dir: &Path,
    transform: impl FnOnce(Config) -> Result<Config>,
) -> Result<(Config, WriteResult)> {
    let parent = config_path.parent().unwrap_or(Path::new("."));
    let existed = parent.exists();
    fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    if !existed {
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let _lock = FileLock::acquire(config_path, true)?;
    let current = load(config_path, default_secret_file)?;
    let updated = transform(current)?;
    validate(&updated)?;
    let result = write_if_changed(config_path, &updated.render(), Some(0o600), Some(backup_dir))?;
    Ok((updated, result))
}

// ---- registry operations (mcp add/set/remove/enable/disable) ----

pub fn normalize_tool_id(value: &str) -> Result<String> {
    let lower = value.trim().to_lowercase();
    let mut slug = String::new();
    let mut dash = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
            dash = false;
        } else if !dash {
            slug.push('-');
            dash = true;
        }
    }
    let mut slug = slug.trim_matches('-').to_string();
    if let Some(rest) = slug.strip_prefix("agent-") {
        slug = rest.to_string();
    }
    if slug.is_empty() {
        return Err("MCP id must contain at least one letter or number".into());
    }
    Ok(format!("agent-{slug}"))
}

pub fn find_tool<'a>(config: &'a Config, id: &str) -> Result<&'a Tool> {
    let normalized = normalize_tool_id(id)?;
    config.find(&normalized).ok_or_else(|| format!("unknown MCP: {normalized}"))
}

pub fn put_tool(mut config: Config, mut tool: Tool, require_new: bool) -> Result<Config> {
    tool.id = normalize_tool_id(&tool.id)?;
    if require_new && config.find(&tool.id).is_some() {
        return Err(format!("MCP already exists: {}", tool.id));
    }
    config.tools.retain(|t| t.id != tool.id);
    config.tools.push(tool);
    config.tools.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(config)
}

pub fn set_enabled(config: Config, id: &str, enabled: bool) -> Result<Config> {
    let mut tool = find_tool(&config, id)?.clone();
    let tool_id = tool.id.clone();
    tool.enabled = enabled;
    let mut updated = put_tool(config, tool, false)?;
    if !enabled {
        updated.routes.clear(&tool_id);
    }
    Ok(updated)
}

pub fn set_mode(config: Config, id: &str, mode: Mode) -> Result<Config> {
    let mut tool = find_tool(&config, id)?.clone();
    tool.mode = mode;
    put_tool(config, tool, false)
}

pub fn remove_tool(mut config: Config, id: &str) -> Result<Config> {
    let normalized = normalize_tool_id(id)?;
    if config.find(&normalized).is_none() {
        return Err(format!("unknown MCP: {normalized}"));
    }
    config.tools.retain(|t| t.id != normalized);
    config.routes.clear(&normalized);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str) -> Tool {
        Tool {
            id: id.into(),
            name: id.into(),
            command: "echo".into(),
            args: vec![],
            required_secrets: vec![],
            apps: Apps::default(),
            env: BTreeMap::new(),
            description: Value::Null,
            enabled: true,
            mode: Mode::Direct,
            idle_seconds: None,
        }
    }

    #[test]
    fn normalizes_ids() {
        assert_eq!(normalize_tool_id(" Tavily Search ").unwrap(), "agent-tavily-search");
        assert_eq!(normalize_tool_id("agent-x").unwrap(), "agent-x");
        assert!(normalize_tool_id("!!").is_err());
    }

    #[test]
    fn rejects_inline_credentials() {
        let mut t = tool("agent-a");
        t.args = vec!["--api-key".into(), "abc".into()];
        let c = Config { tools: vec![t], ..Config::default_for(Path::new("/s")) };
        assert!(validate(&c).unwrap_err().contains("credential-shaped MCP argument 2"));
        let mut t = tool("agent-a");
        t.args = vec!["--api-key=${API_KEY}".into()];
        t.required_secrets = vec!["API_KEY".into()];
        let c = Config { tools: vec![t], ..Config::default_for(Path::new("/s")) };
        assert!(validate(&c).is_ok());
        let mut t = tool("agent-a");
        t.env.insert("API_TOKEN".into(), "x".into());
        let c = Config { tools: vec![t], ..Config::default_for(Path::new("/s")) };
        assert!(validate(&c).is_err());
    }

    #[test]
    fn disabling_clears_routes() {
        let mut c = Config::default_for(Path::new("/s"));
        c.tools.push(tool("agent-s"));
        c.routes.search_default = "agent-s".into();
        let c = set_enabled(c, "s", false).unwrap();
        assert_eq!(c.routes.search_default, "");
        assert!(!c.tools[0].enabled);
    }
}
