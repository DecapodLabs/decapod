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

    let db_path = store.root.join("governance.db");
    assert!(db_path.exists());
}
