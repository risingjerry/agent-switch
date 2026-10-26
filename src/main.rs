//! fusebox: one lightweight control plane for agent secrets, MCP servers,
//! CLIs and Skills.

mod claude;
mod config;
mod doctor;
mod eval;
mod find;
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

const VALUE_OPTS: &[&str] = &[
    "cwd",
    "lang",
    "limit",
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
    "git",
    "ref",
    "skill-path",
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
  skills add-source ID --git URL [--ref R] | --path DIR [--skill-path P]
  skills drop|restore NAME...          take Skills out of the catalog / put them back (files kept)
  find WORDS [--json]                  one catalog: on-demand Skills and MCP servers for a task
  eval [--days 60] [--json]            score find on prompt→Skill pairs from past sessions
  advise [--days 60] [--lang zh] [--json]  suggestions (with commands) for an agent to apply
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
        "advise" => cmd_advise(&ctx, &rest),
        "find" => cmd_find(&ctx, &rest),
        "eval" => cmd_eval(&ctx, &rest),
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
        // New servers start on demand unless asked to be resident.
        mode: if a.flag("resident") { Mode::Direct } else { existing.map_or(Mode::Hub, |t| t.mode) },
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
            a.check(&[
                "name",
                "command",
                "arg",
                "secret",
                "env",
                "app",
                "description",
                "disabled",
                "json",
                "resident",
            ])?;
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
            a.check(&["json", "no-warm"])?;
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
            if sub == "hub" && tool.enabled && !a.flag("no-warm") {
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
        // Resident in PROFILE. --source/--path are optional when the name is
        // unambiguous in the catalog.
        "enable" => {
            a.check(&["source", "path", "no-sync"])?;
            let profile = a.pos(1, "PROFILE")?;
            let hub = ctx.hub()?;
            let names = &a.pos[2..];
            if names.is_empty() {
                return Err("usage: fusebox skills enable PROFILE SKILL... [--source S --path P]".into());
            }
            let catalog = skills::catalog(&ctx.hub_root())?;
            for name in names {
                let (source, path) = match a.one("source") {
                    Some(s) => (s.to_string(), a.one("path").unwrap_or(name).to_string()),
                    None => catalog
                        .iter()
                        .find(|e| &e.name == name)
                        .and_then(|e| e.id.split_once(':').map(|(s, p)| (s.to_string(), p.to_string())))
                        .ok_or_else(|| format!("no skill named {name}; pass --source and --path"))?,
                };
                skills::enable(&hub, profile, name, &source, Some(&path))?;
                print(&format!("{name}: resident in {profile}\n"));
            }
            if !a.flag("no-sync") {
                skills::sync_any(&hub, profile)?;
            }
        }
        // On demand: removed from PROFILE's catalog, still findable by search.
        "disable" => {
            a.check(&["no-sync"])?;
            let profile = a.pos(1, "PROFILE")?;
            let hub = ctx.hub()?;
            let names = &a.pos[2..];
            if names.is_empty() {
                return Err("usage: fusebox skills disable PROFILE SKILL...".into());
            }
            for name in names {
                let changed = skills::disable(&hub, profile, name)?;
                print(&format!(
                    "{name}: {}\n",
                    if changed { format!("on demand in {profile}") } else { format!("was not resident in {profile}") }
                ));
            }
            if !a.flag("no-sync") {
                skills::sync_any(&hub, profile)?;
            }
        }
        // Resident only in --to (taken off --from, default global).
        "add-source" => {
            a.check(&["git", "ref", "path", "skill-path"])?;
            let id = a.pos(1, "ID")?;
            let skill_path = a.one("skill-path").unwrap_or(".");
            let spec = match (a.one("git"), a.one("path")) {
                (Some(url), None) => json!({"type": "git", "url": url, "ref": a.one("ref").unwrap_or("main"),
                    "checkout": format!("vendor/{id}"), "skillPath": skill_path}),
                (None, Some(dir)) => json!({"type": "local", "path": util::expand_user(dir).to_string_lossy(),
                    "skillPath": skill_path}),
                _ => {
                    return Err(
                        "usage: fusebox skills add-source ID (--git URL [--ref REF] | --path DIR) [--skill-path P]"
                            .into(),
                    );
                }
            };
            skills::add_source(&ctx.hub()?, id, spec.clone())?;
            if spec["type"] == "git" {
                print(&format!("added {id}; run `fusebox skills fetch {id}` to clone it\n"));
            } else {
                print(&format!("added {id}; its Skills are on demand now (`fusebox skills`)\n"));
            }
        }
        // Out of the catalog (not resident, not on demand); files are kept.
        "drop" | "restore" => {
            a.check(&["source"])?;
            let names = &a.pos[1..];
            if names.is_empty() {
                return Err(format!("usage: fusebox skills {sub} NAME... [--source S]"));
            }
            let hub = ctx.hub()?;
            let catalog = skills::catalog(&ctx.hub_root())?;
            let mut by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for name in names {
                let source = match a.one("source") {
                    Some(s) => s.to_string(),
                    None if sub == "drop" => {
                        let entry = catalog.iter().find(|e| &e.name == name).ok_or(format!("no Skill named {name}"))?;
                        if !entry.profiles.is_empty() {
                            return Err(format!(
                                "{name} is resident in {}; make it on demand first",
                                entry.profiles.join(", ")
                            ));
                        }
                        entry.id.split_once(':').map(|(s, _)| s.to_string()).unwrap_or_default()
                    }
                    None => skills::excluded_in(&hub, name).ok_or(format!("{name} is not dropped"))?,
                };
                by_source.entry(source).or_default().push(name.clone());
            }
            for (source, names) in by_source {
                for name in skills::set_excluded(&hub, &source, &names, sub == "drop")? {
                    print(&format!("{}: {name} ({source})\n", if sub == "drop" { "dropped" } else { "restored" }));
                }
            }
            print("run `fusebox reconcile` to update agents\n");
        }
        "move" => {
            a.check(&["to", "from"])?;
            let name = a.pos(1, "SKILL")?;
            let to = a.one("to").ok_or("--to PROFILE is required")?;
            let from = a.one("from").unwrap_or("global");
            let hub = ctx.hub()?;
            let (source, path) =
                skills::profile_entry(&hub, from, name).ok_or_else(|| format!("{name} is not resident in {from}"))?;
            skills::enable(&hub, to, name, &source, Some(&path))?;
            skills::disable(&hub, from, name)?;
            skills::sync_any(&hub, to)?;
            skills::sync_any(&hub, from)?;
            print(&format!("{name}: resident in {to}, on demand elsewhere\n"));
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
        "search" => {
            a.check(&["json", "limit"])?;
            let query = a.pos[1..].join(" ");
            if query.trim().is_empty() {
                return Err("usage: fusebox skills search WORDS".into());
            }
            let hub_root = ctx.hub_root();
            let catalog = skills::catalog(&hub_root)?;
            let limit = a.one("limit").and_then(|v| v.parse().ok()).unwrap_or(8);
            // Same ranking as `find`, Skills only.
            let finder = find::Finder::load(&ctx.paths, &hub_root, &ctx.config_path, &ctx.config()?);
            let cwd = std::env::current_dir().unwrap_or_default();
            let hits: Vec<(&skills::CatalogEntry, f64)> = finder
                .hits(&query, Some(&find::project_root(&cwd)), limit + 20)
                .into_iter()
                .filter(|h| h["kind"] == "skill")
                .filter_map(|h| {
                    let e = catalog.iter().find(|e| h["name"].as_str() == Some(e.name.as_str()))?;
                    Some((e, h["score"].as_f64().unwrap_or(0.0)))
                })
                .take(limit)
                .collect();
            if json {
                let rows: Vec<Value> = hits
                    .iter()
                    .map(|(e, score)| {
                        json!({
                            "name": e.name, "id": e.id, "description": e.description, "score": score,
                            "skillMd": e.skill_md.to_string_lossy(), "residentIn": e.profiles,
                        })
                    })
                    .collect();
                print(&(json_ordered(&json!({"query": query, "results": rows})) + "\n"));
            } else if hits.is_empty() {
                print(&format!("No skill matches \"{query}\".\n"));
            } else {
                for (e, _) in &hits {
                    let state = if e.profiles.is_empty() { "on demand".to_string() } else { e.profiles.join(",") };
                    let desc: String = e.description.chars().take(160).collect();
                    print(&format!("{}  [{state}]\n    {desc}\n", e.name));
                }
                print("\nLoad one with: fusebox skills show NAME\n");
            }
        }
        "show" => {
            a.check(&["json"])?;
            let want = a.pos(1, "skill NAME")?;
            let catalog = skills::catalog(&ctx.hub_root())?;
            let entry = catalog
                .iter()
                .find(|e| e.name == want || e.id == want)
                .ok_or_else(|| format!("no skill named {want}; try `fusebox skills search WORDS`"))?;
            let content = std::fs::read_to_string(&entry.skill_md).map_err(|e| e.to_string())?;
            let dir = entry.skill_md.parent().unwrap_or(std::path::Path::new("."));
            if json {
                print(
                    &(json_ordered(&json!({
                        "name": entry.name, "dir": dir.to_string_lossy(), "skillMd": entry.skill_md.to_string_lossy(),
                        "content": content,
                    })) + "\n"),
                );
            } else {
                print(&format!(
                    "<!-- skill {} — resources are relative to {} -->\n{content}",
                    entry.name,
                    dir.display()
                ));
            }
        }
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
            "name": e.name, "profiles": e.profiles, "claude": u.claude, "codex": u.codex, "deepseek": u.deepseek,
            "lastUsed": usage::date_of(u.last_used), "listingChars": usage::listing_chars(&e.skill_md),
        }));
    }
    let total =
        |r: &Value| -> u64 { ["claude", "codex", "deepseek"].iter().map(|k| r[*k].as_u64().unwrap_or(0)).sum() };
    rows.sort_by_key(|r| (std::cmp::Reverse(total(r)), r["name"].as_str().unwrap_or("").to_string()));
    let use_json = |u: &usage::SkillUse| json!({"claude": u.claude, "codex": u.codex, "deepseek": u.deepseek, "lastUsed": usage::date_of(u.last_used)});
    if a.flag("json") {
        let used: BTreeMap<&String, Value> = stats.skills.iter().map(|(k, u)| (k, use_json(u))).collect();
        let mcps: BTreeMap<&String, Value> = stats.mcps.iter().map(|(k, u)| (k, use_json(u))).collect();
        let payload = json!({
            "days": stats.days, "sessions": stats.sessions, "scannedFiles": stats.scanned, "cachedFiles": stats.cached,
            "enabled": rows, "allUsed": used, "mcps": mcps,
        });
        print(&(json_ordered(&payload) + "\n"));
        return Ok(0);
    }
    let n = |k: &str| stats.sessions.get(k).copied().unwrap_or(0);
    print(&format!(
        "Last {} days: {} Claude Code, {} Codex, {} DeepSeek Harness sessions ({} files scanned, {} cached)\n\n",
        stats.days,
        n("claude"),
        n("codex"),
        n("deepseek"),
        stats.scanned,
        stats.cached
    ));
    print(&format!(
        "{:<40} {:>6} {:>6} {:>8}  {:<10} {:>6}  {}\n",
        "SKILL", "CLAUDE", "CODEX", "DEEPSEEK", "LAST USED", "CHARS", "PROFILES"
    ));
    for r in &rows {
        print(&format!(
            "{:<40} {:>6} {:>6} {:>8}  {:<10} {:>6}  {}\n",
            r["name"].as_str().unwrap_or(""),
            r["claude"].as_u64().unwrap_or(0),
            r["codex"].as_u64().unwrap_or(0),
            r["deepseek"].as_u64().unwrap_or(0),
            r["lastUsed"].as_str().unwrap_or(""),
            r["listingChars"].as_u64().unwrap_or(0),
            r["profiles"]
                .as_array()
                .map(|p| p.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","))
                .unwrap_or_default()
        ));
    }
    let unused = rows.iter().filter(|r| total(r) == 0).count();
    print(&format!("\n{unused} of {} enabled skills unused in this window; see `fusebox advise`.\n", rows.len()));
    Ok(0)
}

/// Enabled skills of `profile` with no use in the window, minus the ones a
/// used skill depends on or the profile pins in `keep`.
fn prune_plan(hub: &skills::Hub, stats: &usage::UsageStats, profile: &str) -> Result<(Vec<String>, Vec<Value>)> {
    let enabled = skills::enabled_skills(hub, profile)?;
    let keep = skills::profile_keep(hub, profile);
    let fresh = skills::recently_enabled(hub, profile, stats.days);
    // A skill that another enabled skill names in its SKILL.md is a
    // dependency (e.g. a publisher that calls a design system), even when it
    // is never invoked directly.
    let all_enabled = enabled_everywhere(hub);
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
        if fresh.contains(name) {
            protected.push(json!({"name": name, "reason": "enabled recently"}));
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
    Ok((candidates, protected))
}

fn cmd_skills_prune(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["days", "profile", "apply", "json"])?;
    let profile = a.one("profile").unwrap_or("global");
    let stats = usage::collect(&ctx.paths, days(a)?)?;
    let hub = ctx.hub()?;
    let (candidates, protected) = prune_plan(&hub, &stats, profile)?;
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

// ---- advise ----

/// Short id the hub uses for a tool (`agent-foo-bar` → `foo_bar`).
fn hub_short(id: &str) -> String {
    id.trim_start_matches("agent-").replace('-', "_")
}

/// One actionable suggestion. `commands` are exactly what an agent runs to
/// apply it; `needsHuman` marks steps an agent cannot finish alone.
fn suggestion(
    id: String,
    kind: &str,
    title: String,
    reason: String,
    commands: Vec<String>,
    needs_human: bool,
) -> Value {
    json!({"id": id, "kind": kind, "title": title, "reason": reason, "commands": commands, "needsHuman": needs_human})
}

fn cmd_advise(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["days", "json", "lang"])?;
    // Titles and reasons follow the reader's language; commands never change.
    let zh = a.one("lang") == Some("zh");
    let tr = |en: String, cn: String| if zh { cn } else { en };
    let config = ctx.config()?;
    let report = doctor::run(&config, &ctx.paths)?;
    let stats = usage::collect(&ctx.paths, days(a)?)?;
    let mut out: Vec<Value> = Vec::new();

    if report.blocked() {
        let msgs: Vec<String> =
            report.findings.iter().filter(|f| f.severity == "error").map(|f| f.message.clone()).collect();
        out.push(suggestion(
            "sync.blocked".into(),
            "sync",
            tr("Fix what blocks sync".into(), "先解决阻止同步的问题".into()),
            msgs.join("; "),
            vec![],
            true,
        ));
    } else if !report.changes.is_empty() {
        let n = report.changes.len();
        out.push(suggestion(
            "sync".into(),
            "sync",
            tr(format!("Sync {n} pending change(s) to the agents"), format!("把 {n} 处改动同步到各个 Agent")),
            tr(
                "Agent configs or instructions differ from the Fusebox config.".into(),
                "各 Agent 的配置或说明和 Fusebox 里的不一致。".into(),
            ),
            vec!["fusebox reconcile".into()],
            false,
        ));
    }
    for name in &report.secrets.missing {
        let users: Vec<&str> = config
            .tools
            .iter()
            .filter(|t| t.enabled && t.required_secrets.contains(name))
            .map(|t| t.id.as_str())
            .collect();
        out.push(suggestion(
            format!("secret.{name}"),
            "secret",
            tr(format!("Add the missing secret {name}"), format!("补上缺失的密钥 {name}")),
            tr(
                format!("Needed by {}; only the user can provide the value.", users.join(", ")),
                format!("{} 需要它；值只能由你提供。", users.join("、")),
            ),
            vec![format!("fusebox secret set --stdin {name}   # value via stdin, never as an argument")],
            true,
        ));
    }

    // MCP servers: usage decides always-on vs on-demand vs off.
    let skill_texts: Vec<(String, String)> = ctx
        .hub()
        .map(|h| enabled_everywhere(&h))
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.name, std::fs::read_to_string(&e.skill_md).unwrap_or_default()))
        .collect();
    for tool in config.tools.iter().filter(|t| t.enabled) {
        let mut u = stats.mcps.get(&tool.id).cloned().unwrap_or_default();
        if let Some(h) = stats.mcps.get(&format!("agent-hub/{}", hub_short(&tool.id))) {
            u.claude += h.claude;
            u.codex += h.codex;
            u.deepseek += h.deepseek;
        }
        let available: usize =
            [("claude", tool.apps.claude), ("codex", tool.apps.codex), ("deepseek", tool.apps.deepseek)]
                .iter()
                .filter(|(_, on)| *on)
                .map(|(k, _)| stats.sessions.get(*k).copied().unwrap_or(0))
                .sum();
        let untracked = tool.apps.hermes || tool.apps.claude_desktop;
        let share = if available == 0 { 0.0 } else { u.total() as f64 / available as f64 };
        let pinned: Vec<&str> = skill_texts
            .iter()
            .filter(|(_, text)| text.contains(&format!("mcp__{}__", tool.id)))
            .map(|(n, _)| n.as_str())
            .collect();
        let rename_note = if pinned.is_empty() {
            String::new()
        } else {
            let short = hub_short(&tool.id);
            tr(
                format!(
                    " Skills that name its tools directly ({}) must switch to mcp__agent-hub__{short}__<tool>.",
                    pinned.join(", ")
                ),
                format!(" 直接写了它工具名的 Skill（{}）要改成 mcp__agent-hub__{short}__<工具名>。", pinned.join("、")),
            )
        };
        let name = &tool.name;
        let pct = share * 100.0;
        let used = u.total();
        match tool.mode {
            Mode::Direct if available > 0 && u.total() == 0 => out.push(suggestion(
                format!("mcp.{}.idle", tool.id),
                "mcp",
                tr(format!("{name} was never used in {} days", stats.days), format!("{name} {} 天里一次都没用过", stats.days)),
                tr(
                    format!(
                        "It starts with every session ({available} sessions) but no tool call was seen.{} Switch it to on demand.{rename_note}",
                        if untracked { " Hermes/Claude Desktop usage is not tracked." } else { "" }
                    ),
                    format!(
                        "每个会话都会启动它（共 {available} 个），但从没调用过它的工具。{}建议改成按需。{rename_note}",
                        if untracked { "Hermes / Claude Desktop 里的使用统计不到。" } else { "" }
                    ),
                ),
                vec![format!("fusebox mcp hub {}", tool.id), "fusebox reconcile".into()],
                false,
            )),
            Mode::Direct if available >= 10 && share < 0.2 => out.push(suggestion(
                format!("mcp.{}.hub", tool.id),
                "mcp",
                tr(format!("Serve {name} on demand"), format!("把 {name} 改成按需启动")),
                tr(
                    format!(
                        "Used in {used} of {available} sessions ({pct:.0}%); starting it only when needed saves a process per session.{rename_note}"
                    ),
                    format!(
                        "{available} 个会话里只用到 {used} 次（{pct:.0}%）；用到时再启动，每个会话能省一个后台进程。{rename_note}"
                    ),
                ),
                vec![format!("fusebox mcp hub {}", tool.id), "fusebox reconcile".into()],
                false,
            )),
            Mode::Hub if available >= 10 && share > 0.5 => out.push(suggestion(
                format!("mcp.{}.direct", tool.id),
                "mcp",
                tr(format!("Keep {name} always on"), format!("把 {name} 改回常驻")),
                tr(
                    format!("Used in {pct:.0}% of sessions; a cold start on almost every session costs more than keeping it."),
                    format!("{pct:.0}% 的会话都会用到它；几乎每次都要冷启动，不如一直开着。"),
                ),
                vec![format!("fusebox mcp direct {}", tool.id), "fusebox reconcile".into()],
                false,
            )),
            _ => {}
        }
    }

    // Skills: two states — resident (in a profile's catalog) or on demand
    // (found via `fusebox skills search`). Use is attributed to projects by
    // each session's working directory.
    if let Ok(hub) = ctx.hub() {
        let roots = skills::project_roots(&hub);
        let project_of = |cwd: &str| -> Option<String> {
            roots
                .iter()
                .filter(|(_, root)| !cwd.is_empty() && std::path::Path::new(cwd).starts_with(root))
                .max_by_key(|(_, root)| root.as_os_str().len())
                .map(|(n, _)| n.clone())
        };
        let by_project = |u: &usage::SkillUse| -> BTreeMap<Option<String>, usize> {
            let mut m = BTreeMap::new();
            for (cwd, n) in &u.cwds {
                *m.entry(project_of(cwd)).or_default() += n;
            }
            m
        };
        let enabled_all = enabled_everywhere(&hub);
        let texts: Vec<(String, String)> = enabled_all
            .iter()
            .map(|e| (e.name.clone(), std::fs::read_to_string(&e.skill_md).unwrap_or_default()))
            .collect();
        let used = |name: &str, md: &std::path::Path| stats.lookup(name, dir_name(md).as_deref());
        // A resident skill another used resident skill mentions is a dependency.
        let depended_on = |name: &str| -> Vec<String> {
            texts
                .iter()
                .filter(|(other, text)| other != name && mentions(text, name))
                .filter(|(other, _)| {
                    enabled_all.iter().find(|e| &e.name == other).is_some_and(|e| used(other, &e.skill_md).total() > 0)
                })
                .map(|(o, _)| o.clone())
                .collect()
        };
        for profile in hub.profile_names() {
            let keep = skills::profile_keep(&hub, &profile);
            let fresh = skills::recently_enabled(&hub, &profile, stats.days);
            let mut to_demand = Vec::new();
            let mut kept = Vec::new();
            for (name, md) in skills::enabled_skills(&hub, &profile).unwrap_or_default() {
                let u = used(&name, &md);
                let projects = by_project(&u);
                let here = if profile == "global" {
                    u.total()
                } else {
                    projects.get(&Some(profile.clone())).copied().unwrap_or(0)
                };
                if keep.contains(&name) || fresh.contains(&name) {
                    if here == 0 {
                        kept.push(format!(
                            "{name} ({})",
                            if fresh.contains(&name) {
                                tr("new".into(), "刚启用".into())
                            } else {
                                tr("pinned".into(), "固定保留".into())
                            }
                        ));
                    }
                    continue;
                }
                if here == 0 {
                    let deps = depended_on(&name);
                    if deps.is_empty() {
                        to_demand.push(name);
                    } else {
                        kept.push(format!(
                            "{name} ({})",
                            tr(format!("used by {}", deps.join(", ")), format!("{} 用到它", deps.join("、")))
                        ));
                    }
                    continue;
                }
                // Global skill used in exactly one project and nowhere else:
                // make it resident only there.
                if profile == "global"
                    && projects.len() == 1
                    && u.total() >= 5
                    && let Some(Some(target)) = projects.keys().next()
                {
                    out.push(suggestion(
                        format!("skills.move.{name}"),
                        "skill",
                        tr(format!("Keep {name} resident only in {target}"), format!("{name} 只在 {target} 项目常驻")),
                        tr(
                            format!(
                                "All {} uses in {} days were in {target}; elsewhere it can load on demand.",
                                u.total(),
                                stats.days
                            ),
                            format!(
                                "{} 天里 {} 次使用全在 {target}；其他地方改成按需，用到再调出来。",
                                stats.days,
                                u.total()
                            ),
                        ),
                        vec![format!("fusebox skills move {name} --to {target}")],
                        false,
                    ));
                }
            }
            if !to_demand.is_empty() {
                let scope =
                    if profile == "global" { tr("everywhere".into(), "全局".into()) } else { profile.clone() };
                let commands = vec![format!("fusebox skills disable {profile} {}", to_demand.join(" "))];
                let mut s = suggestion(
                    format!("skills.ondemand.{profile}"),
                    "skill",
                    tr(
                        format!("Make {} unused skill(s) on demand ({scope})", to_demand.len()),
                        format!("{scope} 里 {} 个没用过的 Skill 改成按需", to_demand.len()),
                    ),
                    tr(
                        format!(
                            "Not used here in {} days. On demand they stop costing context in every session; agents still find them with `fusebox skills search`.{}",
                            stats.days,
                            if kept.is_empty() {
                                String::new()
                            } else {
                                format!(" Kept resident: {}.", kept.join("; "))
                            }
                        ),
                        format!(
                            "最近 {} 天在这里没用过。改成按需后不再占每个会话的上下文，需要时 Agent 用 `fusebox skills search` 照样能找到。{}",
                            stats.days,
                            if kept.is_empty() {
                                String::new()
                            } else {
                                format!("继续常驻：{}。", kept.join("；"))
                            }
                        ),
                    ),
                    commands,
                    false,
                );
                s["items"] = json!(to_demand);
                out.push(s);
            }
        }
        // On-demand skills that keep getting loaded: make them resident where used.
        if let Ok(catalog) = skills::catalog(&ctx.hub_root()) {
            let mut seen = BTreeSet::new();
            for e in catalog.iter().filter(|e| e.profiles.is_empty()) {
                if !seen.insert(e.name.clone()) {
                    continue;
                }
                let u = used(&e.name, &e.skill_md);
                // Only loads after it went on demand count; earlier use was
                // while it was resident.
                let loads = match skills::on_demand_since(&hub, &e.name) {
                    Some(date) => u.since(&date),
                    None => u.total(),
                };
                // Used often while resident, rarely since: agents may no
                // longer find it on demand.
                if let Some(date) = skills::on_demand_since(&hub, &e.name)
                    && let Some(drop) = drop_off(&u, &date)
                {
                    let (before, after, days) = drop;
                    let (source, path) =
                        e.id.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
                    out.push(suggestion(
                        format!("skills.dropoff.{}", e.name),
                        "skill",
                        tr(format!("{} may be hard to find on demand", e.name), format!("{} 改成按需后可能不好找了", e.name)),
                        tr(
                            format!(
                                "Used {before} times in the 30 days before it went on demand ({date}), {after} in the {days} days since. \
                                 Add the words people use for it to config/aliases.json, or make it resident."
                            ),
                            format!(
                                "改成按需（{date}）前 30 天用了 {before} 次，之后 {days} 天只用了 {after} 次。\
                                 可以把常说的叫法加进 config/aliases.json，或改回常驻。"
                            ),
                        ),
                        vec![
                            format!("fusebox skills enable global {} --source {source} --path {path}", e.name),
                            "fusebox skills sync-global --prune".into(),
                        ],
                        false,
                    ));
                    continue;
                }
                if loads < 3 {
                    continue;
                }
                let projects = by_project(&u);
                let target = match (projects.len(), projects.keys().next()) {
                    (1, Some(Some(p))) => p.clone(),
                    _ => "global".to_string(),
                };
                let (source, path) =
                    e.id.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
                let sync = if target == "global" {
                    "fusebox skills sync-global --prune".to_string()
                } else {
                    format!("fusebox skills sync {target} --prune")
                };
                out.push(suggestion(
                    format!("skills.resident.{}", e.name),
                    "skill",
                    tr(format!("Make {} resident in {target}", e.name), format!("{} 在 {target} 改成常驻", e.name)),
                    tr(
                        format!(
                            "Loaded on demand in {} sessions; keeping it listed saves a lookup each time.",
                            u.total()
                        ),
                        format!("按需调用了 {} 次；常驻后每次能省一次查找。", u.total()),
                    ),
                    vec![format!("fusebox skills enable {target} {} --source {source} --path {path}", e.name), sync],
                    false,
                ));
            }
        }
    }

    if a.flag("json") {
        let payload = json!({
            "days": stats.days,
            "sessions": stats.sessions,
            "suggestions": out,
            "note": "Show these to the user and apply only what they approve; run each suggestion's commands in order.",
        });
        print(&(json_ordered(&payload) + "\n"));
        return Ok(0);
    }
    if out.is_empty() {
        print(if zh {
            "没有需要调整的，一切都在正常使用。\n"
        } else {
            "Nothing to suggest: everything is synced and in use.\n"
        });
        return Ok(0);
    }
    for (i, s) in out.iter().enumerate() {
        print(&format!(
            "{}. {}\n   {}\n",
            i + 1,
            s["title"].as_str().unwrap_or(""),
            s["reason"].as_str().unwrap_or("")
        ));
        if let Some(items) = s["items"].as_array() {
            let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            print(&format!("   items: {}\n", names.join(", ")));
        }
        for c in s["commands"].as_array().into_iter().flatten() {
            print(&format!("   $ {}\n", c.as_str().unwrap_or("")));
        }
    }
    Ok(0)
}

/// (uses in the 30 days before `since`, uses after, days after) when a Skill
/// used at least 6 times before going on demand has, at least a week later,
/// fallen below 30% of its earlier rate.
fn drop_off(u: &usage::SkillUse, since: &str) -> Option<(usize, usize, usize)> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let days = (0..400).take_while(|k| usage::date_of(now - k * 86_400).as_str() >= since).count().saturating_sub(1);
    let before = u
        .times
        .iter()
        .filter(|t| usage::date_of(**t).as_str() < since && usage::date_of(**t + 30 * 86_400).as_str() >= since)
        .count();
    let after = u.since(since);
    (days >= 7 && before >= 6 && (after as f64) < 0.3 * before as f64 * days.min(30) as f64 / 30.0)
        .then_some((before, after, days))
}

// ---- find: one catalog for Skills and MCP servers ----

fn cmd_eval(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["json", "days"])?;
    let days = a.one("days").and_then(|v| v.parse().ok()).unwrap_or(60);
    let hub_root = ctx.hub_root();
    let finder = find::Finder::load(&ctx.paths, &hub_root, &ctx.config_path, &ctx.config()?);
    let catalog: BTreeSet<String> = skills::catalog(&hub_root)?.into_iter().map(|e| e.name).collect();
    let mut report = eval::run(&ctx.paths, &finder, &catalog, days);
    let history = ctx.paths.cache_dir.join("eval-history.jsonl");
    let previous = std::fs::read_to_string(&history)
        .ok()
        .and_then(|t| t.lines().last().and_then(|l| serde_json::from_str::<Value>(l).ok()));
    let row = json!({"at": skills::now_iso(), "pairs": report["pairs"], "top1": report["top1"], "top5": report["top5"],
        "textTop1": report["textTop1"], "textTop5": report["textTop5"]});
    util::ensure_private_dir(&ctx.paths.cache_dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&history) {
        use std::io::Write;
        let _ = writeln!(f, "{row}");
    }
    report["previous"] = previous.clone().unwrap_or(Value::Null);
    if a.flag("json") {
        print(&(json_ordered(&report) + "\n"));
        return Ok(0);
    }
    let delta = |key: &str| {
        previous.as_ref().and_then(|p| p[key].as_f64()).map_or(String::new(), |before| {
            let d = report[key].as_f64().unwrap_or(0.0) - before;
            if d.abs() < 0.05 { " (=)".into() } else { format!(" ({d:+.1})") }
        })
    };
    print(&format!(
        "{} prompt→Skill pairs from the last {days} days ({} Skills)\n\
         with usage prior: top-1 {}%{}  top-5 {}%{}\n\
         text only:        top-1 {}%{}  top-5 {}%{}\n",
        report["pairs"],
        report["skills"],
        report["top1"],
        delta("top1"),
        report["top5"],
        delta("top5"),
        report["textTop1"],
        delta("textTop1"),
        report["textTop5"],
        delta("textTop5"),
    ));
    let misses = report["misses"].as_array().cloned().unwrap_or_default();
    if !misses.is_empty() {
        print(&format!("\nmissed ({}):\n", misses.len()));
        for m in misses.iter().take(15) {
            print(&format!(
                "  {} ← \"{}\" (got {})\n",
                m["skill"].as_str().unwrap_or(""),
                m["query"].as_str().unwrap_or(""),
                m["got"]
            ));
        }
    }
    Ok(0)
}

fn cmd_find(ctx: &Ctx, a: &Args) -> Result<i32> {
    a.check(&["json", "limit", "cwd"])?;
    let query = a.pos.join(" ");
    if query.trim().is_empty() {
        return Err("usage: fusebox find WORDS".into());
    }
    let limit = a.one("limit").and_then(|v| v.parse().ok()).unwrap_or(5);
    let cwd = a.one("cwd").map(util::expand_user).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let finder = find::Finder::load(&ctx.paths, &ctx.hub_root(), &ctx.config_path, &ctx.config()?);
    let hits = finder.hits(&query, Some(&find::project_root(&cwd)), limit);
    if a.flag("json") {
        print(&(json_ordered(&json!({"query": query, "results": hits})) + "\n"));
        return Ok(0);
    }
    if hits.is_empty() {
        print(&format!("Nothing matches \"{query}\". Try other words (English and Chinese), or `fusebox mcp list`.\n"));
        return Ok(0);
    }
    for v in &hits {
        let desc: String = v["description"].as_str().unwrap_or("").chars().take(150).collect();
        print(&format!(
            "[{}] {}{}\n    {desc}\n    → {}\n",
            v["kind"].as_str().unwrap_or(""),
            v["name"].as_str().unwrap_or(""),
            if v["resident"] == true { " (resident)" } else { "" },
            v["use"].as_str().unwrap_or("")
        ));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_off_needs_a_real_fall() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let since = usage::date_of(now - 10 * 86_400);
        let busy = |n: i64, from: i64| (0..n).map(|i| now - (from + i) * 86_400).collect::<Vec<_>>();
        // 8 uses in the 30 days before, none in the 10 days since: flagged.
        let u = usage::SkillUse { times: busy(8, 12), ..Default::default() };
        assert_eq!(drop_off(&u, &since), Some((8, 0, 10)));
        // Still used at a similar rate: not flagged.
        let mut times = busy(8, 12);
        times.extend(busy(3, 1));
        let u = usage::SkillUse { times, ..Default::default() };
        assert_eq!(drop_off(&u, &since), None);
        // Too recent to judge.
        assert_eq!(
            drop_off(&usage::SkillUse { times: busy(8, 4), ..Default::default() }, &usage::date_of(now - 2 * 86_400)),
            None
        );
    }
}
