// Moved from src/decapod/core/output.rs
use super::*;

#[test]
fn test_compact_line_no_truncation() {
    let input = "short message";
    assert_eq!(compact_line(input, 100), "short message");
}

#[test]
fn test_compact_line_truncation() {
    let input = "this is a very long message that should be truncated when max_chars is small";
    let result = compact_line(input, 20);
    assert!(result.len() <= 23); // 20 + "..."
    assert!(result.ends_with("..."));
}

#[test]
fn test_compact_line_removes_run_hint() {
    let input = "some error (run `decapod help` for details)";
    assert_eq!(compact_line(input, 100), "some error");
}

#[test]
fn test_compact_line_removes_semicolon_hint() {
    let input = "validation failed; run `decapod validate` for more info";
    assert_eq!(compact_line(input, 100), "validation failed");
}

#[test]
fn test_preview_messages_empty() {
    let messages: &[String] = &[];
    assert_eq!(preview_messages(messages, 5, 100), "");
}

#[test]
fn test_preview_messages_single() {
    let messages = vec!["single message".to_string()];
    assert_eq!(preview_messages(&messages, 5, 100), "single message");
}

#[test]
fn test_preview_messages_multiple() {
    let messages = vec![
        "first message".to_string(),
        "second message".to_string(),
        "third message".to_string(),
    ];
    let result = preview_messages(&messages, 2, 100);
    assert!(result.contains("first message"));
    assert!(result.contains("second message"));
    assert!(result.contains("+1 more"));
}

#[test]
fn test_preview_messages_all_shown() {
    let messages = vec!["one".to_string(), "two".to_string()];
    let result = preview_messages(&messages, 5, 100);
    assert!(result.contains("one"));
    assert!(result.contains("two"));
    assert!(!result.contains("more"));
}
