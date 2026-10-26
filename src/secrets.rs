//! `secrets.env` store. Values never reach stdout, stderr or error messages.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::LazyLock;

use regex_lite::Regex;
use serde_json::{Value, json};

use crate::config::{Config, FileLock, SECRET_NAME_RE};
use crate::util::{Result, write_if_changed};

pub const MAX_SECRET_BYTES: usize = 64 * 1024;

static PLAIN_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_./:@%+=,\-]+$").unwrap());

fn decode_double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && matches!(next, '\\' | '"' | '$' | '`')
        {
            out.push(next);
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

fn decode_value(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.starts_with('\'') || value.starts_with('"') {
        let quote = value.chars().next().unwrap();
        if value.len() < 2 || !value.ends_with(quote) {
            return Err("malformed quoted secret value".into());
        }
        let inner = &value[1..value.len() - 1];
        return Ok(if quote == '\'' { inner.to_string() } else { decode_double_quoted(inner) });
    }
    Ok(value.to_string())
}

fn quote_value(value: &str) -> String {
    if PLAIN_VALUE_RE.is_match(value) {
        return value.to_string();
    }
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`");
    format!("\"{escaped}\"")
}

fn line_key(line: &str) -> Option<&str> {
    let mut s = line.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    if let Some(rest) = s.strip_prefix("export ") {
        s = rest.trim();
    }
    let (key, _) = s.split_once('=')?;
    let key = key.trim();
    SECRET_NAME_RE.is_match(key).then_some(key)
}

fn parse(text: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for raw in crate::util::splitlines(text) {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim();
        }
        let Some((key, raw_value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = decode_value(raw_value)?;
        if SECRET_NAME_RE.is_match(key) {
            values.insert(key.to_string(), value);
        }
    }
    Ok(values)
}

fn read_unlocked(path: &Path) -> Result<BTreeMap<String, String>> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("failed to read secret store {}: {}", path.display(), e.kind())),
    }
}

pub fn read_all(path: &Path) -> Result<BTreeMap<String, String>> {
    if !path.parent().is_some_and(Path::exists) {
        return Ok(BTreeMap::new());
    }
    let _lock = FileLock::acquire(path, false)?;
    read_unlocked(path)
}

fn validate_value(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err("secret value must not be empty".into());
    }
    if value.contains('\0') {
        return Err("secret value must not contain NUL bytes".into());
    }
    if value.contains('\r') || value.contains('\n') {
        return Err("secret value must be a single line".into());
    }
    if value.len() > MAX_SECRET_BYTES {
        return Err(format!("secret value exceeds the {MAX_SECRET_BYTES}-byte limit"));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if SECRET_NAME_RE.is_match(name) { Ok(()) } else { Err(format!("invalid secret name: {name}")) }
}

pub fn validate(name: &str, value: &str) -> Result<()> {
    validate_name(name)?;
    validate_value(value)
}

fn ensure_private_parent(path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| format!("chmod failed: {e}"))
}

fn finish_write(path: &Path, lines: &[String]) -> Result<()> {
    let joined = lines.join("\n");
    let trimmed = joined.trim_end();
    let text = if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") };
    write_if_changed(path, &text, Some(0o600), None)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| format!("chmod failed: {e}"))
}

pub fn set(path: &Path, name: &str, value: &str) -> Result<()> {
    validate(name, value)?;
    ensure_private_parent(path)?;
    let _lock = FileLock::acquire(path, true)?;
    let current = crate::util::read_text(path)?;
    let rendered = format!("{name}={}", quote_value(value));
    let mut replaced = false;
    let mut output: Vec<String> = Vec::new();
    for line in crate::util::splitlines(&current) {
        if line_key(line) == Some(name) {
            output.push(rendered.clone());
            replaced = true;
        } else {
            output.push(line.to_string());
        }
    }
    if !replaced {
        if output.last().is_some_and(|l| !l.trim().is_empty()) {
            output.push(String::new());
        }
        output.push(rendered);
    }
    finish_write(path, &output)
}

pub fn get(path: &Path, name: &str) -> Result<String> {
    validate_name(name)?;
    let mut values = read_all(path)?;
    let value = values.remove(name).ok_or_else(|| format!("secret not found: {name}"))?;
    validate_value(&value)?;
    Ok(value)
}

pub fn delete(path: &Path, name: &str) -> Result<()> {
    validate_name(name)?;
    let not_found = || format!("secret not found: {name}");
    if !path.parent().is_some_and(Path::exists) {
        return Err(not_found());
    }
    let _lock = FileLock::acquire(path, true)?;
    if !path.exists() {
        return Err(not_found());
    }
    let current = crate::util::read_text(path)?;
    let lines = crate::util::splitlines(&current);
    let output: Vec<String> = lines.iter().filter(|l| line_key(l) != Some(name)).map(|l| l.to_string()).collect();
    if output.len() == lines.len() {
        return Err(not_found());
    }
    finish_write(path, &output)
}

pub fn list_names(path: &Path) -> Result<Vec<String>> {
    Ok(read_all(path)?.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| k).collect())
}

pub struct Report {
    pub exists: bool,
    pub required: Vec<String>,
    pub missing: Vec<String>,
    pub present: Vec<String>,
    pub stored: Vec<String>,
    pub consumers: BTreeMap<String, Vec<String>>,
    pub path: String,
}

pub fn check(config: &Config) -> Result<Report> {
    let mut required: Vec<String> =
        config.tools.iter().filter(|t| t.enabled).flat_map(|t| t.required_secrets.iter().cloned()).collect();
    required.sort();
    required.dedup();
    let values = read_all(&config.secret_file)?;
    let has = |n: &str| values.get(n).is_some_and(|v| !v.is_empty());
    let consumers = required
        .iter()
        .map(|name| {
            let mut ids: Vec<String> = config
                .tools
                .iter()
                .filter(|t| t.enabled && t.required_secrets.contains(name))
                .map(|t| t.id.clone())
                .collect();
            ids.sort();
            (name.clone(), ids)
        })
        .collect();
    Ok(Report {
        exists: config.secret_file.exists(),
        missing: required.iter().filter(|n| !has(n)).cloned().collect(),
        present: required.iter().filter(|n| has(n)).cloned().collect(),
        stored: values.iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| k.clone()).collect(),
        consumers,
        required,
        path: config.secret_file.to_string_lossy().into_owned(),
    })
}

impl Report {
    pub fn to_value(&self) -> Value {
        json!({
            "path": self.path,
            "exists": self.exists,
            "required": self.required,
            "missing": self.missing,
            "presentNames": self.present,
            "storedNames": self.stored,
            "consumers": self.consumers,
        })
    }
}

// ---- CLI descriptor plumbing ----

fn read_limited(mut reader: impl Read, source: &str) -> Result<String> {
    let mut raw = Vec::new();
    reader
        .by_ref()
        .take((MAX_SECRET_BYTES + 3) as u64)
        .read_to_end(&mut raw)
        .map_err(|e| format!("unable to read secret input {source}: {}", e.kind()))?;
    if raw.ends_with(b"\r\n") {
        raw.truncate(raw.len() - 2);
    } else if raw.ends_with(b"\n") {
        raw.truncate(raw.len() - 1);
    }
    String::from_utf8(raw).map_err(|_| "secret input must be valid UTF-8".into())
}

pub fn read_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err("refusing to read a secret from TTY stdin; use a pipe or inherited file descriptor".into());
    }
    read_limited(stdin.lock(), "stdin")
}

fn dup_fd(fd: i32, purpose: &str) -> Result<OwnedFd> {
    if fd < 3 {
        return Err(format!("--fd must identify an inherited {purpose} descriptor numbered 3 or higher"));
    }
    // SAFETY: the descriptor is only borrowed to duplicate it; validity is
    // checked by the dup itself.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    borrowed.try_clone_to_owned().map_err(|e| {
        format!(
            "unable to duplicate secret {} fd {fd}: {}",
            if purpose == "read" { "input" } else { "output" },
            e.kind()
        )
    })
}

pub fn read_fd(fd: i32) -> Result<String> {
    let owned = dup_fd(fd, "read")?;
    let file = File::from(owned);
    if file.is_terminal() {
        return Err(format!("refusing to read a secret from TTY fd {fd}; use a pipe or inherited file descriptor"));
    }
    read_limited(file, &format!("fd {fd}"))
}

fn same_target(a: &File, b: &BorrowedFd<'_>) -> bool {
    let Ok(left) = a.metadata() else { return false };
    let Ok(dup) = b.try_clone_to_owned() else { return false };
    let Ok(right) = File::from(dup).metadata() else { return false };
    (left.dev(), left.ino(), left.mode() & 0o170000, left.rdev())
        == (right.dev(), right.ino(), right.mode() & 0o170000, right.rdev())
}

/// Write a secret value to inherited descriptor `fd` only. Refuses TTYs and
/// aliases of stdout/stderr.
pub fn write_fd(fd: i32, value: &str) -> Result<()> {
    let owned = dup_fd(fd, "write")?;
    let mut file = File::from(owned);
    if file.is_terminal() {
        return Err(format!("refusing to write a secret to TTY fd {fd}"));
    }
    for std_fd in [1, 2] {
        // SAFETY: stdout/stderr are open for the life of the process.
        let borrowed = unsafe { BorrowedFd::borrow_raw(std_fd) };
        if same_target(&file, &borrowed) {
            return Err(format!("refusing to write a secret through stdout or stderr alias fd {fd}"));
        }
    }
    file.write_all(value.as_bytes()).map_err(|e| format!("unable to write secret output fd {fd}: {}", e.kind()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("as-secret-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("secrets.env")
    }

    #[test]
    fn roundtrip_quotes_and_mode() {
        let path = tmp("rt");
        set(&path, "A_KEY", "plain-value").unwrap();
        set(&path, "B_KEY", "with space $x \"q\" `b` \\").unwrap();
        set(&path, "A_KEY", "new").unwrap();
        assert_eq!(get(&path, "A_KEY").unwrap(), "new");
        assert_eq!(get(&path, "B_KEY").unwrap(), "with space $x \"q\" `b` \\");
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, "A_KEY=new\n\nB_KEY=\"with space \\$x \\\"q\\\" \\`b\\` \\\\\"\n");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(list_names(&path).unwrap(), vec!["A_KEY", "B_KEY"]);
        delete(&path, "A_KEY").unwrap();
        assert!(get(&path, "A_KEY").is_err());
        assert!(delete(&path, "A_KEY").is_err());
    }

    #[test]
    fn rejects_bad_values_without_echoing_them() {
        let path = tmp("bad");
        let err = set(&path, "X_KEY", "line1\nsecretpart").unwrap_err();
        assert!(!err.contains("secretpart"));
        assert!(set(&path, "lower", "v").is_err());
        assert!(set(&path, "X_KEY", "").is_err());
    }

    #[test]
    fn parses_export_and_single_quotes() {
        let m = parse("# c\nexport A='x y'\nB = \"a\\\"b\"\nbad line\nc=lower\n").unwrap();
        assert_eq!(m["A"], "x y");
        assert_eq!(m["B"], "a\"b");
        assert!(!m.contains_key("c"));
    }
}
