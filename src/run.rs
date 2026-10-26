//! `run <tool-id>`: start one MCP backend with only the secrets it declares.

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::config::{Config, SECRET_REFERENCE_RE, SENSITIVE_NAME_RE, Tool};
use crate::secrets;
use crate::util::Result;

/// The environment and argv a tool runs with. Inherited variables that look
/// sensitive, or that share a name with any stored secret, are dropped; only
/// the tool's declared secrets and public env are added back.
pub fn prepare(config: &Config, tool: &Tool, extra_args: &[String]) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let stored = secrets::read_all(&config.secret_file)
        .map_err(|_| format!("Agent Switch could not read the secret store for {}", tool.id))?;
    let missing: Vec<&str> = tool
        .required_secrets
        .iter()
        .filter(|n| stored.get(n.as_str()).is_none_or(String::is_empty))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!("Agent Switch: {} missing required secret(s): {}", tool.id, missing.join(" ")));
    }
    let mut env: BTreeMap<String, String> =
        std::env::vars().filter(|(k, _)| !stored.contains_key(k) && !SENSITIVE_NAME_RE.is_match(k)).collect();
    for name in &tool.required_secrets {
        env.insert(name.clone(), stored[name].clone());
    }
    env.extend(tool.env.clone());
    let expand = |value: &str| -> String {
        SECRET_REFERENCE_RE
            .replace_all(value, |c: &regex_lite::Captures| env.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string()))
            .into_owned()
    };
    let mut argv: Vec<String> = std::iter::once(&tool.command).chain(&tool.args).map(|a| expand(a)).collect();
    argv.extend(extra_args.iter().cloned());
    Ok((argv, env))
}

pub fn command(config: &Config, tool: &Tool, extra_args: &[String]) -> Result<Command> {
    let (argv, env) = prepare(config, tool, extra_args)?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).env_clear().envs(env);
    Ok(cmd)
}

/// Replace the current process with the tool. Only returns on failure.
pub fn exec(config: &Config, tool: &Tool, extra_args: &[String]) -> Result<()> {
    let mut cmd = command(config, tool, extra_args)?;
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let err = cmd.exec();
    if err.kind() == std::io::ErrorKind::NotFound {
        return Err(format!("Agent Switch: {} command not found: {}", tool.id, tool.command));
    }
    Err(format!("Agent Switch: {} failed to start: {err}", tool.id))
}
