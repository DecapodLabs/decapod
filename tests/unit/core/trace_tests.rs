// Moved from src/decapod/core/trace.rs
use super::*;

#[test]
fn test_redact_aws_key() {
    // Preserve full runtime redaction coverage without credential-shaped source.
    let key = format!("{}{}", "AKIA", "X".repeat(16));
    let input = format!("my key is {key} ok");
    let result = redact_string(&input);
    assert!(result.contains("[AWS_KEY_REDACTED]"));
    assert!(!result.contains(&key));
}

#[test]
fn test_redact_github_token() {
    let input = format!("token={}{}", "ghp_", "X".repeat(36));
    let result = redact_string(&input);
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
    let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
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
        "command": format!("export AWS_KEY={}{}", "AKIA", "X".repeat(16)),
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
