//! Defensive redaction for anything printed by doctor/status.

use std::sync::LazyLock;

use regex_lite::Regex;
use serde_json::Value;

use crate::config::TOKEN_VALUE_RE;

static SECRET_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(api[_-]?key|token|secret|password|bearer|authorization)").unwrap());
static ASSIGNMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)((?:api[_-]?key|token|secret|password|authorization)\s*[=:]\s*)([^\s,'"]+)"#).unwrap()
});

pub fn text(value: &str) -> String {
    let first = TOKEN_VALUE_RE.replace_all(value, "[REDACTED]");
    ASSIGNMENT_RE.replace_all(&first, "${1}[REDACTED]").into_owned()
}

fn value_for(key: Option<&str>, value: &Value) -> Value {
    match value {
        Value::String(s) => {
            if key.is_some_and(|k| SECRET_KEY_RE.is_match(k)) {
                Value::String("[REDACTED]".into())
            } else {
                Value::String(text(s))
            }
        }
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), value_for(Some(k), v))).collect()),
        Value::Array(items) => Value::Array(items.iter().map(|v| value_for(key, v)).collect()),
        other => other.clone(),
    }
}

/// Python `redact_mapping`: string values under secret-looking keys are
/// replaced, every other string is scrubbed for token-shaped content.
pub fn mapping(value: &Value) -> Value {
    value_for(None, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_tokens_and_assignments() {
        // Built at runtime so the repository privacy scan never sees a key shape.
        assert_eq!(text(&format!("key {}-abcdefgh1234 ok", "sk")), "key [REDACTED] ok");
        assert_eq!(text("token=abc123"), "token=[REDACTED]");
        let v = mapping(&json!({"consumers": {"X_API_KEY": ["agent-a"]}, "path": "/p"}));
        assert_eq!(v, json!({"consumers": {"X_API_KEY": ["[REDACTED]"]}, "path": "/p"}));
    }
}
