//! Small shared helpers: atomic writes with backups, SHA-256, Python-compatible
//! JSON string encoding and line splitting.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

pub struct WriteResult {
    pub path: PathBuf,
    pub changed: bool,
}

/// Write `text` to `path` only when the bytes differ. Existing content is
/// copied to `backup_dir` first. New files without an explicit mode end up
/// 0600, matching the Python implementation (mkstemp default).
pub fn write_if_changed(path: &Path, text: &str, mode: Option<u32>, backup_dir: Option<&Path>) -> Result<WriteResult> {
    let data = text.as_bytes();
    let fail = |e: io::Error| format!("failed to write {}: {e}", path.display());
    let existing = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(fail(e)),
    };
    if existing.as_deref() == Some(data) {
        return Ok(WriteResult { path: path.to_path_buf(), changed: false });
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(fail)?;
    if let (Some(old), Some(dir)) = (&existing, backup_dir) {
        fs::create_dir_all(dir).map_err(fail)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(fail)?;
        let backup = backup_path_for(path, dir, &sha256_hex(old));
        fs::copy(path, &backup).map_err(fail)?;
        fs::set_permissions(&backup, fs::Permissions::from_mode(0o600)).map_err(fail)?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (tmp_path, mut tmp) = create_temp(parent, &name).map_err(fail)?;
    let result = (|| -> io::Result<()> {
        tmp.write_all(data)?;
        tmp.sync_all()?;
        let mode = match mode {
            Some(m) => Some(m),
            None if existing.is_some() => Some(fs::metadata(path)?.permissions().mode() & 0o7777),
            None => None,
        };
        if let Some(m) = mode {
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(m))?;
        }
        fs::rename(&tmp_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result.map_err(fail)?;
    Ok(WriteResult { path: path.to_path_buf(), changed: true })
}

fn create_temp(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    let seed = std::process::id() as u64 ^ now_nanos();
    for attempt in 0..64u64 {
        let tag = sha256_hex(format!("{seed}-{attempt}").as_bytes());
        let candidate = dir.join(format!(".{name}.{}", &tag[..8]));
        match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("could not create a temporary file"))
}

pub fn now_nanos() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

pub fn backup_path_for(path: &Path, backup_dir: &Path, content_digest: &str) -> PathBuf {
    let resolved = resolve_lenient(path);
    let path_digest = sha256_hex(resolved.to_string_lossy().as_bytes());
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    backup_dir.join(format!("{name}.{}-{}.bak", &path_digest[..10], &content_digest[..12]))
}

/// Like Python's `Path.resolve(strict=False)`: canonicalize the longest
/// existing prefix and append the rest.
pub fn resolve_lenient(path: &Path) -> PathBuf {
    if let Ok(p) = fs::canonicalize(path) {
        return p;
    }
    let absolute =
        if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let mut tail = Vec::new();
    let mut cursor = absolute.as_path();
    while let Some(parent) = cursor.parent() {
        if let Some(name) = cursor.file_name() {
            tail.push(name.to_os_string());
        }
        if let Ok(base) = fs::canonicalize(parent) {
            let mut out = base;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        cursor = parent;
    }
    absolute
}

pub fn ensure_private_dir(path: &Path) {
    let _ = fs::create_dir_all(path);
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

pub fn read_text(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("failed to read {}: {e}", path.display())),
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand_user(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

/// `shutil.which` equivalent.
pub fn which(command: &str) -> Option<PathBuf> {
    if command.contains('/') {
        let p = PathBuf::from(command);
        return is_executable(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(command)).find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Python `str.splitlines()` for the separators that occur in config files.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                out.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                out.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        out.push(&text[start..]);
    }
    out
}

/// Python `json.dumps(value)` for a string (ensure_ascii=True).
pub fn json_str_ascii(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e && (c as u32) != 0x7f => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Python `json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True)`.
pub fn json_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(&sorted(value)).unwrap_or_default()
}

/// Python `json.dumps(value, ensure_ascii=False, indent=2)`: insertion order.
pub fn json_ordered(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

pub fn sorted(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k.clone(), sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// `os.path.relpath(target, base)` for absolute, already-resolved paths.
pub fn relpath(target: &Path, base: &Path) -> PathBuf {
    let t: Vec<_> = target.components().collect();
    let b: Vec<_> = base.components().collect();
    let common = t.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

// ---- SHA-256 (FIPS 180-4), dependency-free ----

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        assert_eq!(sha256_hex(&long), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    #[test]
    fn ascii_json_matches_python() {
        assert_eq!(json_str_ascii("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(json_str_ascii("é😀"), "\"\\u00e9\\ud83d\\ude00\"");
        assert_eq!(json_str_ascii("\u{1}\u{7f}"), "\"\\u0001\u{7f}\"");
    }

    #[test]
    fn splitlines_like_python() {
        assert_eq!(splitlines("a\nb\r\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(splitlines("a\n"), vec!["a"]);
        assert!(splitlines("").is_empty());
    }
}
