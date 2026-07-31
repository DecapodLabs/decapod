use crate::core::error::DecapodError;
use crate::core::events;
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub trace_id: String,
    pub ts: String,
    pub actor: String,
    pub op: String,
    pub request: Value,
    pub response: Value,
}

/// Patterns that detect secrets in string content.
static SECRET_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // AWS Access Key ID
        (
            Regex::new(r"(A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[0-9A-Z]{16}")
                .unwrap(),
            "[AWS_KEY_REDACTED]",
        ),
        // AWS Secret Access Key (40-char base64 near "aws")
        (
            Regex::new(r#"(?i)aws[^=]*=\s*['"]?[0-9a-zA-Z/+=]{40}['"]?"#).unwrap(),
            "[AWS_SECRET_REDACTED]",
        ),
        // GitHub tokens (ghp_, gho_, ghu_, ghs_, ghr_)
        (
            Regex::new(r"(ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9_]{36,255}").unwrap(),
            "[GITHUB_TOKEN_REDACTED]",
        ),
        // Bearer tokens
        (
            Regex::new(r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{20,}").unwrap(),
            "[BEARER_REDACTED]",
        ),
        // PEM private keys (full block)
        (
            Regex::new(r"-----BEGIN (?:RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap(),
            "[PEM_KEY_REDACTED]",
        ),
        // PEM header alone (in case value is truncated)
        (
            Regex::new(r"-----BEGIN (?:RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap(),
            "[PEM_KEY_REDACTED]",
        ),
        // Connection strings (postgres://, mysql://, mongodb://, redis://)
        (
            Regex::new(r#"(?i)(postgres|mysql|mongodb|redis)://[^\s'"]+:[^\s'"]+@[^\s'"]+"#)
                .unwrap(),
            "[CONNECTION_STRING_REDACTED]",
        ),
        // Generic API key assignments
        (
            Regex::new(
                r#"(?i)(api[_-]?key|apikey|api_secret|secret[_-]?key)['"]?\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{20,}['"]?"#,
            )
            .unwrap(),
            "[API_KEY_REDACTED]",
        ),
        // Generic password assignments
        (
            Regex::new(r#"(?i)(password|passwd|pwd)['"]?\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#)
                .unwrap(),
            "[PASSWORD_REDACTED]",
        ),
    ]
});

/// Redact secrets from a plain string value.
pub fn redact_string(input: &str) -> String {
    let mut result = input.to_string();
    for (pattern, replacement) in SECRET_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// Recursively redact a JSON value.
///
/// - Keys that look sensitive (token, secret, password, api_key, authorization)
///   are replaced wholesale with `[REDACTED]`.
/// - String values are scanned for secret patterns (AWS keys, GitHub tokens,
///   bearer tokens, PEM keys, connection strings, API keys, passwords).
pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted_map = Map::new();
            for (key, val) in map {
                let lower_key = key.to_lowercase();
                if lower_key.contains("token")
                    || lower_key.contains("secret")
                    || lower_key.contains("password")
                    || lower_key.contains("api_key")
                    || lower_key.contains("authorization")
                {
                    redacted_map.insert(key, Value::String("[REDACTED]".to_string()));
                } else {
                    redacted_map.insert(key, redact(val));
                }
            }
            Value::Object(redacted_map)
        }
        Value::Array(vec) => Value::Array(vec.into_iter().map(redact).collect()),
        Value::String(s) => Value::String(redact_string(&s)),
        other => other,
    }
}

pub fn append_trace(project_root: &Path, event: TraceEvent) -> Result<(), DecapodError> {
    let redacted_event = TraceEvent {
        trace_id: event.trace_id,
        ts: event.ts,
        actor: event.actor,
        op: event.op,
        request: redact(event.request),
        response: redact(event.response),
    };

    let value = serde_json::to_value(&redacted_event)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    events::append(&project_root.join(".decapod/data"), events::TRACES, &value)?;

    Ok(())
}

pub fn get_last_traces(project_root: &Path, n: usize) -> Result<Vec<String>, DecapodError> {
    let path = events::canonical_db_path(&project_root.join(".decapod/data"));
    if !path.exists() {
        return Ok(vec![]);
    }
    let conn = crate::core::db::db_connect_for_validate(&path.to_string_lossy())?;
    let mut stmt = conn
        .prepare("SELECT payload FROM traces_events ORDER BY seq DESC LIMIT ?1")
        .map_err(DecapodError::RusqliteError)?;
    let rows = stmt
        .query_map([n as i64], |row| row.get::<_, String>(0))
        .map_err(DecapodError::RusqliteError)?;
    let mut lines = Vec::new();
    for row in rows {
        lines.push(row.map_err(DecapodError::RusqliteError)?);
    }
    lines.reverse();
    Ok(lines)
}
#[cfg(test)]
#[path = "../../../tests/unit/core/trace_tests.rs"]
mod tests;
