use crate::core::error::DecapodError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
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
    let trace_path = project_root.join(".decapod/data/traces.jsonl");

    // Ensure parent directory exists
    if let Some(parent) = trace_path.parent() {
        std::fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_path)
        .map_err(DecapodError::IoError)?;

    let redacted_event = TraceEvent {
        trace_id: event.trace_id,
        ts: event.ts,
        actor: event.actor,
        op: event.op,
        request: redact(event.request),
        response: redact(event.response),
    };

    let json = serde_json::to_string(&redacted_event)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    writeln!(file, "{}", json).map_err(DecapodError::IoError)?;

    Ok(())
}

pub fn get_last_traces(project_root: &Path, n: usize) -> Result<Vec<String>, DecapodError> {
    let trace_path = project_root.join(".decapod/data/traces.jsonl");
    if !trace_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(trace_path).map_err(DecapodError::IoError)?;
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = if lines.len() > n { lines.len() - n } else { 0 };
    Ok(lines[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_aws_key() {
        let input = "my key is AKIAIOSFODNN7EXAMPLE ok";
        let result = redact_string(input);
        assert!(result.contains("[AWS_KEY_REDACTED]"));
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_redact_github_token() {
        let input = "token=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let result = redact_string(input);
        assert!(result.contains("[GITHUB_TOKEN_REDACTED]"));
        assert!(!result.contains("ghp_"));
    }

    #[test]
    fn test_redact_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig";
        let result = redact_string(input);
        assert!(result.contains("[BEARER_REDACTED]"));
    }

    #[test]
    fn test_redact_pem_key() {
        let input =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
        let result = redact_string(input);
        assert!(result.contains("[PEM_KEY_REDACTED]"));
        assert!(!result.contains("MIIEpAIBAAKCAQEA"));
    }

    #[test]
    fn test_redact_connection_string() {
        let input = "DATABASE_URL=postgres://user:s3cret@host:5432/db";
        let result = redact_string(input);
        assert!(result.contains("[CONNECTION_STRING_REDACTED]"));
        assert!(!result.contains("s3cret"));
    }

    #[test]
    fn test_redact_password_assignment() {
        let input = r#"password="my_super_secret_pass""#;
        let result = redact_string(input);
        assert!(result.contains("[PASSWORD_REDACTED]"));
    }

    #[test]
    fn test_redact_json_value() {
        let val = serde_json::json!({
            "command": "export AWS_KEY=AKIAIOSFODNN7EXAMPLE",
            "my_token": "should_be_fully_redacted",
            "safe_field": "no secrets here"
        });
        let redacted = redact(val);
        let obj = redacted.as_object().unwrap();
        // Key-based redaction
        assert_eq!(obj["my_token"], "[REDACTED]");
        // Content-based redaction
        let cmd = obj["command"].as_str().unwrap();
        assert!(cmd.contains("[AWS_KEY_REDACTED]"));
        // Safe field untouched
        assert_eq!(obj["safe_field"], "no secrets here");
    }

    #[test]
    fn test_no_false_positive_on_safe_strings() {
        let input = "this is a normal log message with no secrets";
        assert_eq!(redact_string(input), input);
    }
}
