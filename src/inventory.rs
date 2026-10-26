//! `agents` and `clis` reports.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::config::Config;
use crate::doctor;
use crate::instructions::contains_managed_block;
use crate::paths::Paths;
use crate::util::{Result, read_text, which};

pub fn agents(config: &Config, paths: &Paths) -> Result<Vec<Value>> {
    let report = doctor::run(config, paths)?;
    let drift: Vec<&str> = report.changes.iter().map(|c| c.target.as_str()).collect();
    let detected = doctor::detected_apps(paths);
    let codex_managed = read_text(&paths.codex_config)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("model_instructions_file").and_then(|v| v.as_str()).map(String::from))
        .is_some_and(|v| v == paths.codex_instructions.to_string_lossy());
    let defs = [
        (
            "codex",
            "Codex",
            codex_managed,
            &paths.codex_config,
            &paths.codex_instructions,
            &["codex", "instructions.codex"][..],
        ),
        (
            "claude",
            "Claude Code",
            contains_managed_block(&paths.claude_global_instructions),
            &paths.claude_config,
            &paths.claude_global_instructions,
            &["claude", "instructions.claude", "instructions.claude_global"][..],
        ),
        (
            "hermes",
            "Hermes",
            contains_managed_block(&paths.hermes_soul),
            &paths.hermes_config,
            &paths.hermes_soul,
            &["hermes", "instructions.hermes", "instructions.hermes_soul"][..],
        ),
    ];
    Ok(defs
        .into_iter()
        .map(|(id, name, managed, config_path, instruction_path, relevant)| {
            let is_detected = detected.contains(&id);
            let in_sync = is_detected && managed && !drift.iter().any(|t| relevant.contains(t));
            json!({
                "id": id,
                "name": name,
                "detected": is_detected,
                "managed": managed,
                "inSync": in_sync,
                "configPath": config_path.to_string_lossy(),
                "instructionPath": instruction_path.to_string_lossy(),
            })
        })
        .collect())
}

const CLI_TARGETS: [(&str, &str, &str); 15] = [
    ("agent-switch", "Agent Switch", "Agent Switch"),
    ("codex", "Codex", "npm / app"),
    ("claude", "Claude Code", "native installer"),
    ("hermes", "Hermes", "installer"),
    ("lark-cli", "Lark CLI", "npm"),
    ("gh", "GitHub CLI", "Homebrew"),
    ("getnote", "Getnote", "installer"),
    ("firecrawl", "Firecrawl", "npm"),
    ("uv", "uv", "standalone / Homebrew"),
    ("brew", "Homebrew", "Homebrew"),
    ("npm", "npm", "Node.js"),
    ("pipx", "pipx", "Homebrew / pip"),
    ("bun", "Bun", "standalone"),
    ("rustup", "Rustup", "standalone"),
    ("yt-dlp", "yt-dlp", "Homebrew / pipx"),
];

fn version_of(path: &str) -> Option<String> {
    let mut child = Command::new(path)
        .arg("--version")
        .env("PATH", augmented_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    if out.trim().is_empty() {
        out.clear();
        child.stderr.take()?.read_to_string(&mut out).ok()?;
    }
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(240).collect())
}

/// PATH first, then the usual per-user install dirs: apps launched from the
/// Dock get a minimal PATH that misses npm/bun/cargo installs.
fn extra_dirs() -> Vec<std::path::PathBuf> {
    let home = crate::util::home_dir();
    [".local/bin", ".npm-global/bin", ".bun/bin", ".cargo/bin", "Library/pnpm", ".volta/bin"]
        .iter()
        .map(|d| home.join(d))
        .chain(["/opt/homebrew/bin", "/usr/local/bin"].iter().map(std::path::PathBuf::from))
        .collect()
}

fn find_cli(command: &str) -> Option<std::path::PathBuf> {
    which(command).or_else(|| {
        extra_dirs().into_iter().map(|d| d.join(command)).find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
    })
}

/// PATH plus the extra dirs, so `#!/usr/bin/env node` tools can start.
fn augmented_path() -> std::ffi::OsString {
    let mut dirs: Vec<std::path::PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(extra_dirs());
    std::env::join_paths(dirs).unwrap_or_default()
}

pub fn clis() -> Vec<Value> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = CLI_TARGETS
            .iter()
            .map(|&(id, name, manager)| {
                scope.spawn(move || {
                    let path = find_cli(id).map(|p| p.to_string_lossy().into_owned());
                    let version = path.as_deref().and_then(version_of);
                    json!({
                        "id": id, "name": name, "command": id, "manager": manager,
                        "versionArgs": ["--version"], "installed": path.is_some(),
                        "path": path, "version": version,
                    })
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(Value::Null)).collect()
    })
}
