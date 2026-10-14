use decapod::core::store::{Store, StoreKind};
use decapod::plugins::health::{AutonomyTier, HealthState, compute_health, initialize_health_db};
use tempfile::tempdir;

#[test]
fn test_autonomy_tier_display() {
    assert_eq!(AutonomyTier::Untrusted.to_string(), "untrusted");
    assert_eq!(AutonomyTier::Basic.to_string(), "basic");
    assert_eq!(AutonomyTier::Verified.to_string(), "verified");
    assert_eq!(AutonomyTier::Core.to_string(), "core");
}

#[test]
fn test_autonomy_tier_default() {
    let tier = AutonomyTier::default();
    assert_eq!(tier, AutonomyTier::Untrusted);
}

#[test]
fn test_compute_health_no_events() {
    let (state, msg) = compute_health(
        &decapod::plugins::health::Claim {
            id: "test".to_string(),
            subject: "test".to_string(),
            kind: "TODO".to_string(),
            provenance: "test".to_string(),
            created_at: "".to_string(),
        },
        &[],
        1000,
    );

    assert_eq!(state, HealthState::ASSERTED);
    assert!(msg.contains("No proof events"));
}

#[test]
fn test_compute_health_verified() {
    let claim = decapod::plugins::health::Claim {
        id: "test".to_string(),
        subject: "test".to_string(),
        kind: "TODO".to_string(),
        provenance: "test".to_string(),
        created_at: "".to_string(),
    };

    let events = vec![decapod::plugins::health::ProofEvent {
        event_id: "e1".to_string(),
        claim_id: "test".to_string(),
        ts: "1000Z".to_string(),
        surface: "cargo test".to_string(),
        result: "pass".to_string(),
        sla_seconds: 3600,
    }];

    let (state, msg) = compute_health(&claim, &events, 2000);

    assert_eq!(state, HealthState::VERIFIED);
    assert!(msg.contains("Valid proof"));
}

#[test]
fn test_compute_health_contradicted() {
    let claim = decapod::plugins::health::Claim {
        id: "test".to_string(),
        subject: "test".to_string(),
        kind: "TODO".to_string(),
        provenance: "test".to_string(),
        created_at: "".to_string(),
    };

    let events = vec![decapod::plugins::health::ProofEvent {
        event_id: "e1".to_string(),
        claim_id: "test".to_string(),
        ts: "1000Z".to_string(),
        surface: "cargo test".to_string(),
        result: "fail".to_string(),
        sla_seconds: 3600,
    }];

    let (state, msg) = compute_health(&claim, &events, 2000);

    assert_eq!(state, HealthState::CONTRADICTED);
    assert!(msg.contains("failed"));
}

#[test]
fn test_compute_health_stale() {
    let claim = decapod::plugins::health::Claim {
        id: "test".to_string(),
        subject: "test".to_string(),
        kind: "TODO".to_string(),
        provenance: "test".to_string(),
        created_at: "".to_string(),
    };

    let events = vec![decapod::plugins::health::ProofEvent {
        event_id: "e1".to_string(),
        claim_id: "test".to_string(),
        ts: "1000Z".to_string(),
        surface: "cargo test".to_string(),
        result: "pass".to_string(),
        sla_seconds: 60, // 60 seconds SLA
    }];

    // 5000 seconds later - expired
    let (state, msg) = compute_health(&claim, &events, 6000);

    assert_eq!(state, HealthState::STALE);
    assert!(msg.contains("expired SLA"));
}

#[test]
fn test_health_db_init() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };

    initialize_health_db(&store.root).unwrap();

    let db_path = store.root.join("decapod.db");
    assert!(db_path.exists());
}

fn proof_claim() -> decapod::plugins::health::Claim {
    decapod::plugins::health::Claim {
        id: "claim".to_string(),
        subject: "test proof".to_string(),
        kind: "FACT".to_string(),
        provenance: "test".to_string(),
        created_at: "0Z".to_string(),
    }
}

fn proof_event(
    id: &str,
    ts: &str,
    result: &str,
    sla_seconds: i64,
) -> decapod::plugins::health::ProofEvent {
    decapod::plugins::health::ProofEvent {
        event_id: id.to_string(),
        claim_id: "claim".to_string(),
        ts: ts.to_string(),
        surface: "test".to_string(),
        result: result.to_string(),
        sla_seconds,
    }
}

#[test]
fn health_orders_epoch_seconds_numerically_and_allows_later_recovery() {
    let claim = proof_claim();
    let events = [
        proof_event("old", "999Z", "pass", 100),
        proof_event("new", "1000Z", "fail", 100),
    ];
    assert_eq!(
        compute_health(&claim, &events, 1001).0,
        HealthState::CONTRADICTED
    );
    let recovered = [
        proof_event("old", "999Z", "fail", 100),
        proof_event("new", "1000Z", "pass", 100),
    ];
    assert_eq!(
        compute_health(&claim, &recovered, 1001).0,
        HealthState::VERIFIED
    );
}

#[test]
fn health_tied_failure_dominates_independent_of_row_order_or_event_id() {
    let claim = proof_claim();
    let mut events = vec![
        proof_event("z-pass", "1000Z", "pass", 100),
        proof_event("a-fail", "1000Z", "fail", 100),
        proof_event("old", "999Z", "pass", 100),
    ];
    let expected = compute_health(&claim, &events, 1001);
    assert_eq!(expected.0, HealthState::CONTRADICTED);
    events.reverse();
    assert_eq!(compute_health(&claim, &events, 1001), expected);
    events.rotate_left(1);
    assert_eq!(compute_health(&claim, &events, 1001), expected);
}

#[test]
fn health_tied_passes_require_every_sla_and_include_exact_expiry_second() {
    let claim = proof_claim();
    let mut events = [
        proof_event("long", "1000Z", "pass", 3600),
        proof_event("short", "1000Z", "pass", 60),
    ];
    assert_eq!(
        compute_health(&claim, &events, 1060).0,
        HealthState::VERIFIED
    );
    let expired = compute_health(&claim, &events, 1061);
    assert_eq!(expired.0, HealthState::STALE);
    events.reverse();
    assert_eq!(compute_health(&claim, &events, 1061), expired);
}

#[test]
fn health_unknown_latest_or_tied_result_cannot_reuse_a_pass() {
    let claim = proof_claim();
    for unknown_ts in ["1000Z", "1001Z"] {
        for result in ["", "unknown", "PASS", "skipped"] {
            let mut events = [
                proof_event("pass", "1000Z", "pass", 3600),
                proof_event("unknown", unknown_ts, result, 3600),
            ];
            let expected = compute_health(&claim, &events, 1002);
            assert_eq!(expected.0, HealthState::ASSERTED);
            events.reverse();
            assert_eq!(compute_health(&claim, &events, 1002), expected);
        }
    }
}

#[test]
fn health_rejects_unorderable_or_future_timestamps_without_reviving_old_pass() {
    let claim = proof_claim();
    for timestamp in [
        "",
        "Z",
        "bad",
        "1000ZZ",
        "-1Z",
        "+1000Z",
        "1000.5Z",
        "9223372036854775808Z",
        "1001Z",
    ] {
        let events = [
            proof_event("old", "999Z", "pass", 3600),
            proof_event("invalid", timestamp, "pass", i64::MAX),
        ];
        assert_eq!(
            compute_health(&claim, &events, 1000).0,
            HealthState::ASSERTED,
            "timestamp {timestamp:?} must not verify"
        );
    }
}

#[test]
fn health_rejects_negative_and_overflowing_latest_slas() {
    let claim = proof_claim();
    for sla in [-1, i64::MIN, i64::MAX] {
        let events = [proof_event("invalid", "1000Z", "pass", sla)];
        assert_eq!(
            compute_health(&claim, &events, 1000).0,
            HealthState::ASSERTED
        );
    }
    let zero = [proof_event("zero", "1000Z", "pass", 0)];
    assert_eq!(compute_health(&claim, &zero, 1000).0, HealthState::VERIFIED);
    assert_eq!(compute_health(&claim, &zero, 1001).0, HealthState::STALE);
}

#[test]
fn health_expired_latest_pass_does_not_fall_back_to_older_long_lived_pass() {
    let events = [
        proof_event("old", "999Z", "pass", 3600),
        proof_event("latest", "1000Z", "pass", 1),
    ];
    assert_eq!(
        compute_health(&proof_claim(), &events, 1002).0,
        HealthState::STALE
    );
}

#[test]
fn health_cannot_verify_another_claims_proof() {
    let mut event = proof_event("wrong-claim", "1000Z", "pass", 3600);
    event.claim_id = "somewhere-else".to_string();
    assert_eq!(
        compute_health(&proof_claim(), &[event], 1001).0,
        HealthState::ASSERTED
    );
}
