//! Compact output rendering helpers for CLI surfaces.
//!
//! Keeps command result output bounded and readable while preserving signal.

/// Collapse newlines/extra whitespace and bound length for terminal display.
pub fn compact_line(input: &str, max_chars: usize) -> String {
    let mut collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(idx) = collapsed.find(" (run `") {
        collapsed.truncate(idx);
    }
    if let Some(idx) = collapsed.find("; run `") {
        collapsed.truncate(idx);
    }
    let mut chars = collapsed.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", preview)
    } else {
        preview
    }
}

/// Render up to `max_items` messages with compact formatting.
pub fn preview_messages(messages: &[String], max_items: usize, max_chars: usize) -> String {
    if messages.is_empty() {
        return String::new();
    }
    let shown = messages
        .iter()
        .take(max_items)
        .map(|m| compact_line(m, max_chars))
        .collect::<Vec<_>>()
        .join(" | ");
    if messages.len() > max_items {
        format!("{} (+{} more)", shown, messages.len() - max_items)
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
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
}
