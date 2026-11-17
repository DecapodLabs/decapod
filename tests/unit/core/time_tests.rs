// Moved from src/decapod/core/time.rs
use super::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedTimestamp {
    #[serde(with = "serde_utc")]
    timestamp: OffsetDateTime,
}

#[test]
fn persisted_timestamp_preserves_utc_string_and_fractional_precision() {
    for (input, expected) in [
        ("2026-11-17T03:04:05+02:00", "2026-11-17T01:04:05Z"),
        ("2026-11-17T01:04:05.1Z", "2026-11-17T01:04:05.100Z"),
        ("2026-11-17T01:04:05.1234Z", "2026-11-17T01:04:05.123400Z"),
        (
            "2026-11-17T01:04:05.1234567Z",
            "2026-11-17T01:04:05.123456700Z",
        ),
    ] {
        let decoded: PersistedTimestamp =
            serde_json::from_value(serde_json::json!({"timestamp":input})).unwrap();
        assert_eq!(decoded.timestamp.offset(), UtcOffset::UTC);
        assert_eq!(
            serde_json::to_value(&decoded).unwrap()["timestamp"],
            expected
        );
        assert_eq!(
            format_utc(decoded.timestamp, false, false).unwrap(),
            expected.replace('Z', "+00:00")
        );
    }
    for invalid in [
        "1794913254Z",
        "2026-11-17 01:04:05",
        "2026-02-30T01:04:05Z",
        "invalid",
        "9999-12-31T23:59:59-23:59",
    ] {
        assert!(
            serde_json::from_value::<PersistedTimestamp>(serde_json::json!({"timestamp":invalid}))
                .is_err()
        );
    }
}

#[test]
fn event_timestamp_retains_nine_digits_and_exact_instant() {
    let instant = parse_rfc3339("2026-11-17T03:04:05.12+02:00").unwrap();
    assert_eq!(
        format_rfc3339_nanos(instant).unwrap(),
        "2026-11-17T01:04:05.120000000Z"
    );
    assert_eq!(
        parse_rfc3339(&format_rfc3339_nanos(instant).unwrap()).unwrap(),
        instant
    );
    assert_eq!(
        format_rfc3339_nanos(OffsetDateTime::UNIX_EPOCH).unwrap(),
        "1970-01-01T00:00:00.000000000Z"
    );
}

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

#[test]
fn out_of_range_utc_conversion_is_rejected_without_panicking() {
    let value = OffsetDateTime::parse("9999-12-31T23:59:59-23:59", &Rfc3339).unwrap();
    assert!(parse_rfc3339("9999-12-31T23:59:59-23:59").is_err());
    assert!(format_rfc3339_nanos(value).is_err());
    assert!(serde_json::to_value(PersistedTimestamp { timestamp: value }).is_err());
}
