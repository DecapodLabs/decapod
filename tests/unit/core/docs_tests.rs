// Moved from src/decapod/core/docs.rs
use super::truncate_chars;

#[test]
fn truncate_chars_respects_char_boundaries() {
    let input = "alpha — beta";
    assert_eq!(truncate_chars(input, 7), "alpha —...");
}
