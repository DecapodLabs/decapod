//! Compact output rendering helpers for CLI surfaces.
//!
//! Keeps command result output bounded and readable while preserving signal.

/// Collapse newlines/extra whitespace and bound length for terminal display.
pub fn compact_line(input: &str, max_chars: usize) -> String {
    let collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
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
