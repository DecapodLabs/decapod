// Moved from src/decapod/core/time.rs
use super::*;

#[test]
fn test_now_epoch_z_format() {
    let result = now_epoch_z();
    assert!(result.ends_with('Z'));
    let numeric_part = result.trim_end_matches('Z');
    assert!(numeric_part.parse::<u64>().is_ok());
}

#[test]
fn test_new_event_id_is_unique() {
    let id1 = new_event_id();
    let id2 = new_event_id();
    assert_ne!(id1, id2);
}

#[test]
fn test_new_event_id_is_valid_ulid() {
    let id = new_event_id();
    assert!(crate::core::ulid::is_valid(&id));
}

#[test]
fn test_command_envelope_basic() {
    let envelope = command_envelope("test", "ok", serde_json::json!({}));
    assert_eq!(envelope["cmd"], "test");
    assert_eq!(envelope["status"], "ok");
    assert!(envelope["ts"].is_string());
    assert!(envelope["event_id"].is_string());
    assert_eq!(envelope["envelope_version"], "1.0.0");
}

#[test]
fn test_command_envelope_with_extra() {
    let extra = serde_json::json!({"key": "value", "count": 42});
    let envelope = command_envelope("test", "ok", extra);
    assert_eq!(envelope["key"], "value");
    assert_eq!(envelope["count"], 42);
}
