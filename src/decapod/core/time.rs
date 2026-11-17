//! Shared timestamp/event helpers for deterministic envelopes.

use serde_json::Value as JsonValue;
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

/// Parse external timestamps into the UTC storage domain.
pub fn parse_rfc3339(value: &str) -> Result<OffsetDateTime, time::Error> {
    OffsetDateTime::parse(value, &Rfc3339)?
        .checked_to_offset(UtcOffset::UTC)
        .ok_or(time::error::ConversionRange.into())
}

// Preserve the established 0/3/6/9 fractional digits and UTC spellings. The
// default serializer for OffsetDateTime is a tuple, not our existing wire format.
fn format_utc(
    value: OffsetDateTime,
    fixed_nanos: bool,
    use_z: bool,
) -> Result<String, time::Error> {
    let value = value
        .checked_to_offset(UtcOffset::UTC)
        .ok_or(time::error::ConversionRange)?;
    let mut result = value.format(time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second]"
    ))?;
    let nanos = value.nanosecond();
    let width = if fixed_nanos || !nanos.is_multiple_of(1_000) {
        9
    } else if !nanos.is_multiple_of(1_000_000) {
        6
    } else if nanos != 0 {
        3
    } else {
        0
    };
    if width != 0 {
        result.push('.');
        let fraction = format!("{nanos:09}");
        result.push_str(&fraction[..width]);
    }
    result.push_str(if use_z { "Z" } else { "+00:00" });
    Ok(result)
}

pub fn now_rfc3339() -> String {
    format_utc(OffsetDateTime::now_utc(), false, false)
        .expect("system UTC timestamp is formattable")
}

/// Fixed-width Dactyl event timestamps remain lexically ordered.
pub fn format_rfc3339_nanos(value: OffsetDateTime) -> Result<String, time::Error> {
    format_utc(value, true, true)
}

/// Compatibility for persisted and remote Task/KnowledgeEntry/Decision JSON.
pub mod serde_utc {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &OffsetDateTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer
            .serialize_str(&format_utc(*value, false, true).map_err(serde::ser::Error::custom)?)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<OffsetDateTime, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse_rfc3339(&value).map_err(serde::de::Error::custom)
    }
}

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
