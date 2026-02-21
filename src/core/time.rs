//! Shared timestamp/event helpers for deterministic envelopes.

use serde_json::Value as JsonValue;
use ulid::Ulid;

/// Returns unix-epoch seconds with `Z` suffix (e.g. `1771220592Z`).
pub fn now_epoch_z() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

pub fn new_event_id() -> String {
    Ulid::new().to_string()
}

/// Standard command response envelope shape used across CLI surfaces.
pub fn command_envelope(cmd: &str, status: &str, extra: JsonValue) -> JsonValue {
    let mut base = serde_json::json!({
        "envelope_version": "1.0.0",
        "ts": now_epoch_z(),
        "event_id": new_event_id(),
        "cmd": cmd,
        "status": status
    });
    if let (Some(base_obj), Some(extra_obj)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }
    base
}

#[cfg(test)]
mod tests {
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
        assert!(ulid::Ulid::from_string(&id).is_ok());
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
}
