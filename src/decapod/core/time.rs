//! Shared timestamp/event helpers for deterministic envelopes.

use serde_json::Value as JsonValue;

/// Returns unix-epoch seconds with `Z` suffix (e.g. `1771220592Z`).
pub fn now_epoch_z() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}Z")
}

pub fn new_event_id() -> String {
    crate::core::ulid::new_ulid()
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
#[path = "../../../tests/unit/core/time_tests.rs"]
mod tests;
