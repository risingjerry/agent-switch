//! fusebox: one lightweight control plane for agent secrets, MCP servers,
//! CLIs and Skills.

mod config;
mod doctor;
mod hub;
mod imports;
mod instructions;
mod inventory;
mod paths;
mod redact;
mod render;
mod run;
mod secrets;
mod skills;
mod usage;
mod util;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::{Value, json};

use config::{Apps, Config, Mode, Tool};
use paths::Paths;
use util::{Result, json_ordered, json_pretty};

const VALUE_OPTS: [&str; 17] = [
    "home",
    "user-home",
    "config",
    "name",
    "command",
    "arg",
    "secret",
    "env",
    "app",
    "description",
    "fd",
    "source",
    "path",
    "to",
    "days",
    "profile",
    "output",
];

#[derive(Default)]
struct Args {
    pos: Vec<String>,
    opts: BTreeMap<String, Vec<String>>,
}

impl Args {
    fn parse(argv: &[String]) -> Result<Args> {
        let mut args = Args::default();
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            if a == "--" {
                args.pos.extend(argv[i + 1..].iter().cloned());
                break;
            }
            if let Some(body) = a.strip_prefix("--") {
                let (name, inline) = match body.split_once('=') {
                    Some((n, v)) => (n.to_string(), Some(v.to_string())),
                    None => (body.to_string(), None),
                };
                if VALUE_OPTS.contains(&name.as_str()) {
                    let value = match inline {
                        Some(v) => v,
                        None => {
                            i += 1;
                            argv.get(i).cloned().ok_or_else(|| format!("--{name} requires a value"))?
                        }
                    };
                    args.opts.entry(name).or_default().push(value);
                } else {
                    if inline.is_some() {
                        return Err(format!("--{name} does not take a value"));
                    }
                    args.opts.entry(name).or_default();
                }
            } else {
                args.pos.push(a.clone());
            }
            i += 1;
        }
        Ok(args)
    }
    fn flag(&self, name: &str) -> bool {
        self.opts.contains_key(name)
    }
    fn one(&self, name: &str) -> Option<&str> {
        self.opts.get(name).and_then(|v| v.last()).map(String::as_str)
    }
    fn many(&self, name: &str) -> Vec<String> {
        self.opts.get(name).cloned().unwrap_or_default()
    }
    fn pos(&self, i: usize, what: &str) -> Result<&str> {
        self.pos.get(i).map(String::as_str).ok_or_else(|| format!("missing {what}"))
    }
    fn check(&self, allowed: &[&str]) -> Result<()> {
        const GLOBAL: [&str; 3] = ["home", "user-home", "config"];
        match self.opts.keys().find(|k| !allowed.contains(&k.as_str()) && !GLOBAL.contains(&k.as_str())) {
            Some(k) => Err(format!("unrecognized option --{k}")),
            None => Ok(()),
        }
    }
}

struct Ctx {
    paths: Paths,
    config_path: PathBuf,
}

impl Ctx {
    fn new(args: &Args) -> Ctx {
        let paths = Paths::new(args.one("home"), args.one("user-home"));
        let config_path = args.one("config").map(util::expand_user).unwrap_or_else(|| paths.config_file.clone());
        Ctx { paths, config_path }
    }
    fn config(&self) -> Result<Config> {
        config::load(&self.config_path, &self.paths.secrets_file)
    }
    fn update(&self, f: impl FnOnce(Config) -> Result<Config>) -> Result<(Config, util::WriteResult)> {
        config::update(&self.config_path, &self.paths.secrets_file, &self.paths.backup_dir, f)
    }
    fn hub(&self) -> Result<skills::Hub> {
        skills::Hub::load(&self.hub_root())
    }
    fn hub_root(&self) -> PathBuf {
        self.config().ok().and_then(|c| c.skill_hub).unwrap_or_else(|| instructions::skill_hub_dir(&self.paths))
    }
}

fn print(s: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(s.as_bytes());
}

fn json_or_line(json: bool, payload: Value, line: &str) {
    if json { print(&(json_pretty(&payload) + "\n")) } else { print(&format!("{line}\n")) }
}

const USAGE: &str = "fusebox — one control plane for agent secrets, MCP servers, CLIs and Skills

Usage: fusebox [--home DIR] [--user-home DIR] [--config FILE] <command>

  status | doctor [--json] [--strict]   show drift, missing secrets, findings
  reconcile [--json] [--dry-run]        write native configs, wrappers, instructions
  agents | clis [--json]                detected agents / installed CLIs
  mcp list|add|set|remove|enable|disable|import
  mcp hub ID | mcp direct ID            serve an MCP lazily through the hub, or natively
  secret set --stdin|--fd N NAME | get --fd N NAME | list | delete NAME
  skills [list] [--json]                skill inventory
  skills status|fetch|promote|enable|disable|sync|sync-global|update
  skills usage [--days 60] [--json]     what each agent actually used
  skills prune [--days 60] [--profile global] [--apply]
  run ID [ARGS...]                      start an MCP with only its secrets (used by wrappers)
  hub APP                               lazy MCP proxy (used by the agent-hub entry)
  write-default-config
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match dispatch(&argv) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fusebox: {e}");
            2
        }
    };
    std::process::exit(code);
}

fn dispatch(argv: &[String]) -> Result<i32> {
    // `run` passes everything after the tool id through untouched.
    if let Some(pos) = argv.iter().position(|a| a == "run")
        && let Ok(globals) = Args::parse(&argv[..pos])
        && globals.pos.is_empty()
    {
        let ctx = Ctx::new(&globals);
        let id = argv.get(pos + 1).ok_or("missing tool id")?;
        return cmd_run(&ctx, id, &argv[pos + 2..]);
    }
    let args = Args::parse(argv)?;
    if args.flag("version") {
        print(&format!("fusebox {}\n", env!("CARGO_PKG_VERSION")));
        return Ok(0);
    }
    if args.flag("help") || args.pos.is_empty() {
        print(USAGE);
        return Ok(if args.pos.is_empty() && !args.flag("help") { 2 } else { 0 });
    }
    let ctx = Ctx::new(&args);
    let rest = Args { pos: args.pos[1..].to_vec(), opts: args.opts.clone() };
    match args.pos[0].as_str() {
        "doctor" | "status" => cmd_doctor(&ctx, &rest),
        "reconcile" => cmd_reconcile(&ctx, &rest),
        "agents" => {
            rest.check(&["json"])?;
            let agents = inventory::agents(&ctx.config()?, &ctx.paths)?;
            if rest.flag("json") {
                print(&(json_ordered(&json!({"agents": agents})) + "\n"));
            } else {
                for a in agents {
                    let state = match (a["detected"].as_bool(), a["inSync"].as_bool(), a["managed"].as_bool()) {
                        (Some(false), _, _) => "not-detected",
                        (_, Some(true), _) => "managed",
                        (_, _, Some(true)) => "needs-sync",
                        _ => "unmanaged",
                    };
                    print(&format!("{}: {state}\n", a["id"].as_str().unwrap_or("")));
                }
            }
            Ok(0)
        }
        "clis" => {
            rest.check(&["json"])?;
            let items = inventory::clis();
            if rest.flag("json") {
                print(&(json_ordered(&json!({"clis": items})) + "\n"));
            } else {
                for c in items {
                    let state = c["version"].as_str().or(c["path"].as_str()).unwrap_or("not-installed");
                    print(&format!("{}: {state}\n", c["id"].as_str().unwrap_or("")));
                }
            }
            Ok(0)
        }
        "mcp" => cmd_mcp(&ctx, &rest),
        "secret" => cmd_secret(&ctx, &rest),
        "skills" => cmd_skills(&ctx, &rest),
        "hub" => {
            let app = rest.pos.first().cloned().unwrap_or_else(|| "claude".into());
            hub::serve(ctx.paths.clone(), ctx.config_path.clone(), app)?;
            Ok(0)
        }
        "write-default-config" => {
            let config = ctx.config()?;
            util::ensure_private_dir(&ctx.paths.agent_home);
            let r =
                util::write_if_changed(&ctx.paths.config_file, &config.render(), None, Some(&ctx.paths.backup_dir))?;
            print(&format!("{} {}\n", if r.changed { "wrote" } else { "unchanged" }, r.path.display()));
            Ok(0)
        }
        "preview" | "dashboard" => Err(format!("`{}` was removed in 0.4 together with CC Switch support", args.pos[0])),
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

fn cmd_doctor(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["json", "strict", "no-ccswitch"])?;
    let report = doctor::run(&ctx.config()?, &ctx.paths)?;
    if a.flag("json") {
        print(&(json_pretty(&report.to_value()) + "\n"))
    } else {
        print(&report.human())
    }
    let failed = report.blocked() || !report.changes.is_empty() || !report.secrets.missing.is_empty();
    Ok(if a.flag("strict") && failed { 1 } else { 0 })
}

fn cmd_reconcile(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["json", "dry-run", "no-ccswitch"])?;
    let config = ctx.config()?;
    if a.flag("dry-run") {
        let report = doctor::run(&config, &ctx.paths)?;
        if a.flag("json") {
            print(&(json_pretty(&report.to_value()) + "\n"))
        } else {
            print(&report.human())
        }
        return Ok(if report.blocked() { 1 } else { 0 });
    }
    let (summary, post) = doctor::apply(&config, &ctx.paths)?;
    if a.flag("json") {
        print(&(json_pretty(&json!({"summary": summary.to_value(), "post": post.to_value()})) + "\n"));
    } else {
        print(&format!("changed={} unchanged={} blocked={}\n", summary.changed, summary.unchanged, summary.blocked));
        print(&post.human());
    }
    Ok(if post.blocked() { 1 } else { 0 })
}

// ---- mcp ----

fn parse_env(entries: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for entry in entries {
        let (name, value) = entry.split_once('=').ok_or_else(|| format!("--env must use NAME=VALUE: {entry}"))?;
        if !config::ENV_NAME_RE.is_match(name) {
            return Err(format!("invalid environment name: {name}"));
        }
        if config::SENSITIVE_NAME_RE.is_match(name) {
            return Err(format!("sensitive environment name must be declared with --secret, not --env: {name}"));
        }
        out.insert(name.to_string(), value.to_string());
    }
    Ok(out)
}

fn tool_from_args(a: &Args, existing: Option<&Tool>) -> Result<Tool> {
    let id = config::normalize_tool_id(a.pos(1, "MCP id")?)?;
    let command = a.one("command").ok_or("--command is required")?.to_string();
    let apps_list = a.many("app");
    for app in &apps_list {
        if !config::APP_NAMES.contains(&app.as_str()) {
            return Err(format!("invalid --app: {app}"));
        }
    }
    let mut apps = if apps_list.is_empty() { Apps::default() } else { Apps::none() };
    for app in &apps_list {
        apps.set(app, true);
    }
    let secrets: BTreeSet<String> = a.many("secret").into_iter().collect();
    Ok(Tool {
        name: a.one("name").map(String::from).unwrap_or_else(|| id.trim_start_matches("agent-").to_string()),
        command,
        args: a.many("arg"),
        required_secrets: secrets.into_iter().collect(),
        apps,
        env: parse_env(&a.many("env"))?,
        description: a.one("description").map_or(Value::Null, |d| json!(d)),
        enabled: !a.flag("disabled"),
        mode: existing.map_or(Mode::Direct, |t| t.mode),
        idle_seconds: existing.and_then(|t| t.idle_seconds),
        id,
    })
}

fn cmd_mcp(ctx: &Ctx, a: &Args) -> Result<i32> {
    let sub = a.pos(0, "mcp subcommand (list|add|set|remove|enable|disable|import|hub|direct)")?;
    let json = a.flag("json");
    match sub {
        "list" => {
            a.check(&["json"])?;
            let config = ctx.config()?;
            if json {
                let tools: Vec<Value> = config
                    .tools
                    .iter()
                    .map(|t| {
                        let mut v = t.to_public();
                        v["toolCount"] = json!(hub::cached_tool_count(&ctx.paths, &t.id));
                        v
                    })
                    .collect();
                print(&(json_pretty(&json!({"mcps": tools})) + "\n"));
            } else {
                for t in &config.tools {
                    let apps = t.apps.enabled_names();
                    let apps = if apps.is_empty() { "none".into() } else { apps.join(",") };
                    let mode = if t.mode == Mode::Hub { " (hub)" } else { "" };
                    print(&format!(
                        "{}: {}{mode} [{apps}] {} {}\n",
                        t.id,
                        if t.enabled { "enabled" } else { "disabled" },
                        t.command,
                        t.args.join(" ")
                    ));
                }
            }
            Ok(0)
        }
        "add" | "set" => {
            a.check(&["name", "command", "arg", "secret", "env", "app", "description", "disabled", "json"])?;
            let require_new = sub == "add";
            let current = ctx.config()?;
            let id = config::normalize_tool_id(a.pos(1, "MCP id")?)?;
            let tool = tool_from_args(a, current.find(&id))?;
            let (updated, r) = ctx.update(|c| config::put_tool(c, tool, require_new))?;
            let stored = updated.find(&id).unwrap();
            let verb = if require_new { "added" } else { "saved" };
            json_or_line(json, json!({"changed": r.changed, "mcp": stored.to_public()}), &format!("{verb} {id}"));
            Ok(0)
        }
        "remove" => {
            a.check(&["json"])?;
            let id = config::normalize_tool_id(a.pos(1, "MCP id")?)?;
            let (_, r) = ctx.update(|c| config::remove_tool(c, &id))?;
            json_or_line(json, json!({"changed": r.changed, "id": id}), &format!("removed {id}"));
            Ok(0)
        }
        "enable" | "disable" | "hub" | "direct" => {
            a.check(&["json"])?;
            let id = config::normalize_tool_id(a.pos(1, "MCP id")?)?;
            let (updated, r) = ctx.update(|c| match sub {
                "enable" => config::set_enabled(c, &id, true),
                "disable" => config::set_enabled(c, &id, false),
                "hub" => config::set_mode(c, &id, Mode::Hub),
                _ => config::set_mode(c, &id, Mode::Direct),
            })?;
            let tool = updated.find(&id).unwrap();
            let verb = match sub {
                "enable" => "enabled",
                "disable" => "disabled",
                "hub" => "hub-served",
                _ => "direct",
            };
            let mut line = format!("{verb} {id}");
            if sub == "hub" && tool.enabled {
                match hub::warm(&ctx.paths, &updated, tool) {
                    Ok(n) => line.push_str(&format!("; cached {n} tool(s); run `fusebox reconcile`")),
                    Err(e) => {
                        line.push_str(&format!("; tool list not cached yet ({e}); the hub retries on first start"))
                    }
                }
            }
            json_or_line(json, json!({"changed": r.changed, "mcp": tool.to_public()}), &line);
            Ok(0)
        }
        "import" => cmd_mcp_import(ctx, a),
        other => Err(format!("unknown mcp subcommand: {other}")),
    }
}

fn cmd_mcp_import(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["app", "dry-run", "adopt", "no-ccswitch", "json"])?;
    let mut apps = a.many("app");
    if apps.is_empty() {
        apps = config::APP_NAMES.iter().map(|s| s.to_string()).collect();
    }
    let config = ctx.config()?;
    let discovery = imports::discover(&ctx.paths, &apps)?;
    let plan = imports::plan(&config, &discovery.mcps)?;
    let skipped: Vec<Value> =
        discovery.skipped.iter().map(|s| json!({"app": s.app, "id": s.id, "reason": s.reason})).collect();
    let mut payload = json!({
        "discovered": discovery.mcps.len() + discovery.skipped.len(),
        "supported": discovery.mcps.len(),
        "skipped": skipped,
    });
    let merge = |p: &mut Value, extra: Value| {
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
    };
    merge(&mut payload, plan.public());
    if a.flag("dry-run") {
        payload["dryRun"] = json!(true);
        json_or_line(
            a.flag("json"),
            payload,
            &format!(
                "would import {} MCP(s), merge {}, and leave {} unsupported MCP(s) untouched",
                plan.imported.len(),
                plan.merged.len(),
                discovery.skipped.len()
            ),
        );
        return Ok(0);
    }
    let stored = secrets::read_all(&ctx.paths.secrets_file)?;
    let conflicts: Vec<&String> =
        plan.secrets.iter().filter(|(k, v)| stored.get(*k).is_some_and(|s| s != *v)).map(|(k, _)| k).collect();
    if !conflicts.is_empty() {
        return Err(format!(
            "import would overwrite existing secret name(s): {}",
            conflicts.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    for (name, value) in &plan.secrets {
        secrets::validate(name, value)?;
    }
    for (name, value) in &plan.secrets {
        secrets::set(&ctx.paths.secrets_file, name, value)?;
    }
    let natives = &discovery.mcps;
    let (updated, r) = ctx.update(|c| imports::plan(&c, natives).map(|p| p.config))?;
    let mut adopted = 0;
    let mut sync = Value::Null;
    if a.flag("adopt") {
        if doctor::run(&updated, &ctx.paths)?.blocked() {
            return Err("MCPs were imported but adoption is blocked; no source entry was removed".into());
        }
        adopted = imports::adopt(&ctx.paths, natives)?;
        let (summary, post) = doctor::apply(&updated, &ctx.paths)?;
        if post.blocked() {
            return Err("MCPs were imported but reconciliation is blocked; run fusebox doctor".into());
        }
        sync = summary.to_value();
    }
    payload["changed"] = json!(r.changed);
    payload["adoptedSourceFiles"] = json!(adopted);
    payload["reconcile"] = sync;
    payload["note"] = json!(if a.flag("adopt") {
        "source entries were backed up and replaced by managed projections"
    } else {
        "source entries are preserved until --adopt is explicitly requested"
    });
    json_or_line(
        a.flag("json"),
        payload,
        &format!(
            "imported {} MCP(s), merged {}, stored {} secret name(s), and left {} unsupported MCP(s) untouched",
            plan.imported.len(),
            plan.merged.len(),
            plan.secrets.len(),
            discovery.skipped.len()
        ),
    );
    Ok(0)
}

// ---- secret ----

fn cmd_secret(ctx: &Ctx, a: &Args) -> Result<i32> {
    let file = &ctx.paths.secrets_file;
    let fd = || -> Result<Option<i32>> {
        a.one("fd").map(|v| v.parse::<i32>().map_err(|_| format!("--fd must be an integer: {v}"))).transpose()
    };
    match a.pos(0, "secret subcommand (set|get|list|delete)")? {
        "set" => {
            a.check(&["stdin", "fd"])?;
            let name = a.pos(1, "secret NAME")?;
            if a.pos.len() > 2 {
                return Err("positional secret values are not supported; use --stdin NAME or --fd N NAME".into());
            }
            let value = match (a.flag("stdin"), fd()?) {
                (true, None) => secrets::read_stdin()?,
                (false, Some(n)) => secrets::read_fd(n)?,
                _ => return Err("choose exactly one secret source: --stdin or --fd N".into()),
            };
            secrets::set(file, name, &value)?;
            print(&format!("set {name} in {}\n", file.display()));
        }
        "get" => {
            a.check(&["fd"])?;
            let n = fd()?.ok_or("--fd N is required; secrets are never written to stdout")?;
            let name = a.pos(1, "secret NAME")?;
            secrets::write_fd(n, &secrets::get(file, name)?)?;
        }
        "list" => {
            a.check(&["json"])?;
            let names = secrets::list_names(file)?;
            if a.flag("json") {
                print(&(json_ordered(&json!({"path": file.to_string_lossy(), "names": names})) + "\n"));
            } else {
                for n in names {
                    print(&format!("{n}\n"));
                }
            }
        }
        "delete" => {
            a.check(&[])?;
            let name = a.pos(1, "secret NAME")?;
            secrets::delete(file, name)?;
            print(&format!("deleted {name} from {}\n", file.display()));
        }
        other => return Err(format!("unknown secret subcommand: {other}")),
    }
    Ok(0)
}

// ---- run ----

fn cmd_run(ctx: &Ctx, id: &str, extra: &[String]) -> Result<i32> {
    let config = ctx.config()?;
    let tool = config::find_tool(&config, id)?.clone();
    match run::exec(&config, &tool, extra) {
        Ok(()) => Ok(0),
        Err(e) => {
            eprintln!("{e}");
            Ok(if e.contains("command not found") { 127 } else { 78 })
        }
    }
}

// ---- skills ----

fn cmd_skills(ctx: &Ctx, a: &Args) -> Result<i32> {
    let json = a.flag("json");
    let sub = a.pos.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => {
            a.check(&["json"])?;
            let report = skills::report(&ctx.hub_root())?;
            if json {
                print(&(json_ordered(&report) + "\n"));
            } else {
                for s in report["skills"].as_array().into_iter().flatten() {
                    print(&format!(
                        "{}: {} ({})\n",
                        s["name"].as_str().unwrap_or(""),
                        s["status"].as_str().unwrap_or(""),
                        s["source"].as_str().unwrap_or("")
                    ));
                }
            }
        }
        "status" => {
            a.check(&[])?;
            print(&(skills::status_lines(&ctx.hub()?, &ctx.paths.agent_home).join("\n") + "\n"));
        }
        "fetch" | "update" => {
            a.check(&["json"])?;
            let events = skills::fetch(&ctx.hub()?, if sub == "fetch" { &a.pos[1..] } else { &[] })?;
            if json && sub == "update" {
                let report = skills::report(&ctx.hub_root())?;
                print(
                    &(json_ordered(&json!({"updated": true, "message": events.join("\n"), "report": report})) + "\n"),
                );
            } else {
                print(&events.iter().map(|e| format!("{e}\n")).collect::<String>());
            }
        }
        "promote" => {
            a.check(&["to"])?;
            let source = a.pos(1, "SOURCE")?;
            let to = a.one("to").ok_or("--to REF is required")?;
            skills::promote(&ctx.hub()?, source, to)?;
            print(&format!("promoted {source} to {to}; run `fusebox skills fetch {source}` to refresh the lock\n"));
        }
        "enable" => {
            a.check(&["source", "path"])?;
            let (profile, name) = (a.pos(1, "PROFILE")?, a.pos(2, "SKILL")?);
            let source = a.one("source").ok_or("--source is required")?;
            skills::enable(&ctx.hub()?, profile, name, source, a.one("path"))?;
            print(&format!("enabled {name} in profile {profile}; run `fusebox skills sync {profile} --prune`\n"));
        }
        "disable" => {
            a.check(&[])?;
            let (profile, name) = (a.pos(1, "PROFILE")?, a.pos(2, "SKILL")?);
            if skills::disable(&ctx.hub()?, profile, name)? {
                print(&format!("disabled {name} in profile {profile}; run `fusebox skills sync {profile} --prune`\n"));
            } else {
                print(&format!("{name} was not enabled in profile {profile}\n"));
            }
        }
        "sync" => {
            a.check(&["prune", "dry-run"])?;
            let profile = a.pos(1, "PROFILE")?;
            let hub = ctx.hub()?;
            let events = if profile == "global" {
                skills::sync_global(&hub, a.flag("dry-run"), a.flag("prune"))?
            } else {
                skills::sync_profile(&hub, profile, a.flag("dry-run"), a.flag("prune"))?
            };
            print(&events.iter().map(|e| format!("{e}\n")).collect::<String>());
            if a.flag("dry-run") {
                print("dry-run: no changes written\n");
            }
        }
        "sync-global" => {
            a.check(&["prune", "dry-run"])?;
            let events = skills::sync_global(&ctx.hub()?, a.flag("dry-run"), a.flag("prune"))?;
            print(&events.iter().map(|e| format!("{e}\n")).collect::<String>());
            if a.flag("dry-run") {
                print("dry-run: no changes written\n");
            }
        }
        "usage" => return cmd_skills_usage(ctx, a),
        "prune" => return cmd_skills_prune(ctx, a),
        other => return Err(format!("unknown skills subcommand: {other}")),
    }
    Ok(0)
}

fn days(a: &Args) -> Result<u64> {
    a.one("days").map_or(Ok(60), |d| d.parse().map_err(|_| format!("--days must be a number: {d}")))
}

struct Enabled {
    name: String,
    profiles: Vec<String>,
    skill_md: PathBuf,
}

fn enabled_everywhere(hub: &skills::Hub) -> Vec<Enabled> {
    let mut by_name: BTreeMap<String, Enabled> = BTreeMap::new();
    for profile in hub.profile_names() {
        for (name, skill_md) in skills::enabled_skills(hub, &profile).unwrap_or_default() {
            by_name
                .entry(name.clone())
                .or_insert(Enabled { name, profiles: Vec::new(), skill_md })
                .profiles
                .push(profile.clone());
        }
    }
    by_name.into_values().collect()
}

fn dir_name(skill_md: &std::path::Path) -> Option<String> {
    skill_md.parent()?.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn cmd_skills_usage(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["days", "json"])?;
    let stats = usage::collect(&ctx.paths, days(a)?)?;
    let hub = ctx.hub()?;
    let mut rows = Vec::new();
    for e in enabled_everywhere(&hub) {
        let u = stats.lookup(&e.name, dir_name(&e.skill_md).as_deref());
        rows.push(json!({
            "name": e.name, "profiles": e.profiles, "claude": u.claude, "codex": u.codex,
            "lastUsed": usage::date_of(u.last_used), "listingChars": usage::listing_chars(&e.skill_md),
        }));
    }
    rows.sort_by_key(|r| {
        (
            std::cmp::Reverse(r["claude"].as_u64().unwrap_or(0) + r["codex"].as_u64().unwrap_or(0)),
            r["name"].as_str().unwrap_or("").to_string(),
        )
    });
    if a.flag("json") {
        let used: BTreeMap<&String, Value> = stats
            .skills
            .iter()
            .map(|(k, u)| (k, json!({"claude": u.claude, "codex": u.codex, "lastUsed": usage::date_of(u.last_used)})))
            .collect();
        let payload = json!({
            "days": stats.days, "sessions": stats.sessions, "scannedFiles": stats.scanned, "cachedFiles": stats.cached,
            "enabled": rows, "allUsed": used,
        });
        print(&(json_ordered(&payload) + "\n"));
        return Ok(0);
    }
    print(&format!(
        "Last {} days: {} Claude sessions, {} Codex sessions ({} files scanned, {} cached)\n\n",
        stats.days,
        stats.sessions.get("claude").unwrap_or(&0),
        stats.sessions.get("codex").unwrap_or(&0),
        stats.scanned,
        stats.cached
    ));
    print(&format!(
        "{:<40} {:>6} {:>6}  {:<10} {:>6}  {}\n",
        "SKILL", "CLAUDE", "CODEX", "LAST USED", "CHARS", "PROFILES"
    ));
    for r in &rows {
        print(&format!(
            "{:<40} {:>6} {:>6}  {:<10} {:>6}  {}\n",
            r["name"].as_str().unwrap_or(""),
            r["claude"].as_u64().unwrap_or(0),
            r["codex"].as_u64().unwrap_or(0),
            r["lastUsed"].as_str().unwrap_or(""),
            r["listingChars"].as_u64().unwrap_or(0),
            r["profiles"]
                .as_array()
                .map(|p| p.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","))
                .unwrap_or_default()
        ));
    }
    let unused = rows.iter().filter(|r| r["claude"] == 0 && r["codex"] == 0).count();
    print(&format!("\n{unused} of {} enabled skills unused in this window; see `fusebox skills prune`.\n", rows.len()));
    Ok(0)
}

fn cmd_skills_prune(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["days", "profile", "apply", "json"])?;
    let profile = a.one("profile").unwrap_or("global");
    let stats = usage::collect(&ctx.paths, days(a)?)?;
    let hub = ctx.hub()?;
    let enabled = skills::enabled_skills(&hub, profile)?;
    let keep = skills::profile_keep(&hub, profile);
    // A skill that another enabled skill names in its SKILL.md is a
    // dependency (e.g. a publisher that calls a design system), even when it
    // is never invoked directly.
    let all_enabled = enabled_everywhere(&hub);
    let texts: Vec<(String, String)> = all_enabled
        .iter()
        .map(|e| (e.name.clone(), std::fs::read_to_string(&e.skill_md).unwrap_or_default()))
        .collect();
    let mut candidates = Vec::new();
    let mut protected = Vec::new();
    for (name, skill_md) in &enabled {
        let u = stats.lookup(name, dir_name(skill_md).as_deref());
        if u.total() > 0 {
            continue;
        }
        if keep.contains(name) {
            protected.push(json!({"name": name, "reason": "listed in profile keep"}));
            continue;
        }
        let users: Vec<&str> = texts
            .iter()
            .filter(|(other, text)| other != name && mentions(text, name))
            .filter(|(other, _)| {
                let e = all_enabled.iter().find(|e| &e.name == other).unwrap();
                stats.lookup(other, dir_name(&e.skill_md).as_deref()).total() > 0
            })
            .map(|(other, _)| other.as_str())
            .collect();
        if !users.is_empty() {
            protected.push(json!({"name": name, "reason": format!("used by {}", users.join(", "))}));
            continue;
        }
        candidates.push(name.clone());
    }
    let apply = a.flag("apply");
    let mut events = Vec::new();
    if apply && !candidates.is_empty() {
        for name in &candidates {
            skills::disable(&hub, profile, name)?;
        }
        events = if profile == "global" {
            skills::sync_global(&hub, false, true)?
        } else {
            skills::sync_profile(&hub, profile, false, true)?
        };
    }
    if a.flag("json") {
        let payload = json!({
            "profile": profile, "days": stats.days, "applied": apply, "disabled": candidates,
            "protected": protected, "events": events,
        });
        print(&(json_ordered(&payload) + "\n"));
        return Ok(0);
    }
    if candidates.is_empty() {
        print(&format!(
            "Nothing to prune in profile {profile}: every enabled skill was used in the last {} days.\n",
            stats.days
        ));
    } else {
        print(&format!(
            "{} {} unused skill(s) in profile {profile} (last {} days):\n",
            if apply { "Disabled" } else { "Would disable" },
            candidates.len(),
            stats.days
        ));
        for c in &candidates {
            print(&format!("  - {c}\n"));
        }
    }
    for p in &protected {
        print(&format!("  kept {} ({})\n", p["name"].as_str().unwrap_or(""), p["reason"].as_str().unwrap_or("")));
    }
    if !apply && !candidates.is_empty() {
        print("Dry run. Re-run with --apply to disable them (source files are never deleted).\n");
    }
    Ok(0)
}

/// Whole-word mention of a skill name (names contain `-`, so a plain
/// substring check would match prefixes like `github` in `github-auth`).
fn mentions(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    text.match_indices(name).any(|(i, m)| {
        let before = text[..i].chars().next_back();
        let after = text[i + m.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}
