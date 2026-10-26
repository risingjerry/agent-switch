use std::path::{Path, PathBuf};

use crate::util::{expand_user, home_dir};

#[derive(Clone, Debug)]
pub struct Paths {
    pub agent_home: PathBuf,
    pub config_file: PathBuf,
    pub secrets_file: PathBuf,
    pub codex_instructions: PathBuf,
    pub claude_instructions: PathBuf,
    pub hermes_instructions: PathBuf,
    pub wrapper_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub user_home: PathBuf,
    pub claude_config: PathBuf,
    pub claude_global_instructions: PathBuf,
    pub claude_desktop_config: PathBuf,
    pub codex_config: PathBuf,
    pub hermes_config: PathBuf,
    pub hermes_soul: PathBuf,
}

impl Paths {
    pub fn new(agent_home: Option<&str>, user_home: Option<&str>) -> Paths {
        let home = match agent_home {
            Some(h) => expand_user(h),
            None => match std::env::var("AGENT_SWITCH_HOME") {
                Ok(v) if !v.is_empty() => expand_user(&v),
                _ => home_dir().join(".config").join("agent-switch"),
            },
        };
        let user = user_home.map(expand_user).unwrap_or_else(home_dir);
        let instructions = home.join("instructions");
        Paths {
            config_file: home.join("config.json"),
            secrets_file: home.join("secrets.env"),
            codex_instructions: instructions.join("AGENTS.md"),
            claude_instructions: instructions.join("CLAUDE.md"),
            hermes_instructions: instructions.join("HERMES.md"),
            wrapper_dir: home.join("mcp").join("bin"),
            backup_dir: home.join("backups"),
            cache_dir: match std::env::var("AGENT_SWITCH_CACHE") {
                Ok(v) if !v.is_empty() => expand_user(&v),
                _ => user.join(".cache").join("agent-switch"),
            },
            claude_config: user.join(".claude.json"),
            claude_global_instructions: user.join(".claude").join("CLAUDE.md"),
            claude_desktop_config: user
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
            codex_config: user.join(".codex").join("config.toml"),
            hermes_config: user.join(".hermes").join("config.yaml"),
            hermes_soul: user.join(".hermes").join("SOUL.md"),
            agent_home: home,
            user_home: user,
        }
    }

    /// True when the target user home is the real home of this process, so
    /// PATH lookups and /Applications describe the same machine.
    pub fn targets_real_user(&self) -> bool {
        canonical(&self.user_home) == canonical(&home_dir())
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
