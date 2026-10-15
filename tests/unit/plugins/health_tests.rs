// Moved from src/decapod/plugins/health.rs
use super::*;
use crate::core::store::StoreKind;
use tempfile::tempdir;

#[test]
fn summary_observes_canonical_watcher_event_without_jsonl() {
    let dir = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    let ts = crate::core::time::now_epoch_z();
    events::append(
        &store.root,
        events::WATCHER,
        &serde_json::json!({
            "event_id": "watcher-health",
            "ts": ts,
            "event_type": "watcher.run",
            "actor": "watcher"
        }),
    )
    .unwrap();

    let summary = get_summary(&store).unwrap();
    assert_eq!(summary.watcher_last_run.as_deref(), Some(ts.as_str()));
    assert!(!summary.watcher_stale);
    assert!(!dir.path().join("watcher.events.jsonl").exists());
}

fn health_connection() -> (tempfile::TempDir, db::Connection) {
    let dir = tempdir().unwrap();
    let conn = db::Connection::open(dir.path().join("health-test.db")).unwrap();
    conn.execute(schemas::HEALTH_DB_SCHEMA_CLAIMS, []).unwrap();
    conn.execute(schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS, [])
        .unwrap();
    conn.execute(schemas::HEALTH_DB_SCHEMA_HEALTH_CACHE, [])
        .unwrap();
    (dir, conn)
}

fn insert_health_claim(conn: &db::Connection, id: &str, subject: &str) {
    conn.execute(
        "INSERT INTO claims(id, subject, kind, provenance, created_at) VALUES(?1, ?2, 'FACT', NULL, '0Z')",
        params![id, subject],
    )
    .unwrap();
}

fn insert_health_proof(
    conn: &db::Connection,
    event_id: &str,
    claim_id: &str,
    ts: &str,
    result: &str,
    sla: i64,
) {
    conn.execute(
        "INSERT INTO proof_events(event_id, claim_id, ts, surface, result, sla_seconds) VALUES(?1, ?2, ?3, 'test', ?4, ?5)",
        params![event_id, claim_id, ts, result, sla],
    )
    .unwrap();
}

#[test]
fn connection_health_uses_proofs_without_reading_or_writing_cache() {
    let (_dir, conn) = health_connection();
    insert_health_claim(&conn, "proof", "Proof label");
    insert_health_proof(&conn, "pass", "proof", "1000Z", "pass", 10);

    let fresh = derive_health_from_connection(&conn, "proof", 1001)
        .unwrap()
        .unwrap();
    assert_eq!(fresh.1, HealthState::VERIFIED);
    let cache_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM health_cache", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cache_count, 0);

    conn.execute(
        "INSERT INTO health_cache(claim_id, computed_state, reason, updated_at) VALUES('proof', 'CONTRADICTED', 'stale cache', '999Z')",
        [],
    )
    .unwrap();
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .unwrap(),
        fresh
    );
    conn.execute(
        "UPDATE health_cache SET computed_state = 'VERIFIED', reason = 'poisoned', updated_at = '1000Z' WHERE claim_id = 'proof'",
        [],
    )
    .unwrap();
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1011)
            .unwrap()
            .unwrap()
            .1,
        HealthState::STALE
    );
    insert_health_proof(&conn, "fail", "proof", "1001Z", "fail", 10);
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1002)
            .unwrap()
            .unwrap()
            .1,
        HealthState::CONTRADICTED
    );
    let cached: (String, String) = conn
        .query_row(
            "SELECT computed_state, reason FROM health_cache WHERE claim_id = 'proof'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(cached, ("VERIFIED".to_string(), "poisoned".to_string()));
}

#[test]
fn connection_health_does_not_need_a_cache_table_or_end_callers_transaction() {
    let (_dir, conn) = health_connection();
    conn.execute("DROP TABLE health_cache", []).unwrap();
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    insert_health_claim(&conn, "proof", "label");
    insert_health_proof(&conn, "pass", "proof", "1000Z", "pass", 10);
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .unwrap()
            .1,
        HealthState::VERIFIED
    );
    assert!(!conn.is_autocommit());
    assert!(!conn.has_table("health_cache").unwrap());
    conn.execute_batch("ROLLBACK").unwrap();
    assert!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .is_none()
    );
}

#[test]
fn connection_health_runs_on_a_read_only_connection() {
    let (dir, conn) = health_connection();
    insert_health_claim(&conn, "proof", "label");
    insert_health_proof(&conn, "pass", "proof", "1000Z", "pass", 10);
    drop(conn);
    let conn = db::Connection::open_with_flags(
        dir.path().join("health-test.db"),
        db::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .unwrap()
            .1,
        HealthState::VERIFIED
    );
}

#[test]
fn connection_health_prioritizes_exact_ids_and_rejects_ambiguous_subjects() {
    let (_dir, conn) = health_connection();
    insert_health_claim(&conn, "other", "exact");
    insert_health_claim(&conn, "exact", "unique label");
    insert_health_claim(&conn, "duplicate-a", "duplicate label");
    insert_health_claim(&conn, "duplicate-b", "duplicate label");
    insert_health_proof(&conn, "failure", "other", "1000Z", "fail", 60);
    insert_health_proof(&conn, "pass", "exact", "1000Z", "pass", 60);

    let exact = derive_health_from_connection(&conn, "exact", 1001)
        .unwrap()
        .unwrap();
    assert_eq!(exact.0, "exact");
    assert_eq!(exact.1, HealthState::VERIFIED);
    assert_eq!(
        derive_health_from_connection(&conn, "unique label", 1001)
            .unwrap()
            .unwrap(),
        exact
    );
    let error = derive_health_from_connection(&conn, "duplicate label", 1001).unwrap_err();
    assert!(error.to_string().contains("HEALTH_CLAIM_AMBIGUOUS"));
    assert!(
        derive_health_from_connection(&conn, "missing", 1001)
            .unwrap()
            .is_none()
    );
}

#[test]
fn connection_health_distinguishes_missing_tables_from_invalid_schemas() {
    let dir = tempdir().unwrap();
    let conn = db::Connection::open(dir.path().join("missing.db")).unwrap();
    assert!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .is_none()
    );
    assert!(!conn.has_table("claims").unwrap());
    conn.execute(schemas::HEALTH_DB_SCHEMA_CLAIMS, []).unwrap();
    insert_health_claim(&conn, "proof", "label");
    let missing_proofs = derive_health_from_connection(&conn, "proof", 1001)
        .unwrap()
        .unwrap();
    assert_eq!(missing_proofs.1, HealthState::ASSERTED);
    assert!(missing_proofs.2.contains("table is missing"));
    assert!(!conn.has_table("proof_events").unwrap());
    conn.execute("CREATE TABLE proof_events (claim_id TEXT)", [])
        .unwrap();
    conn.execute("INSERT INTO proof_events VALUES ('proof')", [])
        .unwrap();
    assert!(matches!(
        derive_health_from_connection(&conn, "proof", 1001),
        Err(error::DecapodError::StorageError(_))
    ));

    let bad_claims = db::Connection::open(dir.path().join("bad-claims.db")).unwrap();
    bad_claims
        .execute("CREATE TABLE claims (id TEXT PRIMARY KEY)", [])
        .unwrap();
    bad_claims
        .execute("INSERT INTO claims VALUES ('proof')", [])
        .unwrap();
    assert!(matches!(
        derive_health_from_connection(&bad_claims, "proof", 1001),
        Err(error::DecapodError::StorageError(_))
    ));
}

#[test]
fn connection_health_cached_verification_cannot_replace_missing_events() {
    let (_dir, conn) = health_connection();
    insert_health_claim(&conn, "proof", "label");
    conn.execute(
        "INSERT INTO health_cache(claim_id, computed_state, updated_at) VALUES('proof', 'VERIFIED', '1000Z')",
        [],
    )
    .unwrap();
    assert_eq!(
        derive_health_from_connection(&conn, "proof", 1001)
            .unwrap()
            .unwrap()
            .1,
        HealthState::ASSERTED
    );
}

#[test]
fn public_health_get_and_list_use_current_proofs_and_cache_resolved_ids() {
    let dir = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    initialize_health_db(&store.root).unwrap();
    policy::initialize_policy_db(&store.root).unwrap();
    add_claim(&store, "proof", "label", "FACT", "test").unwrap();
    record_proof(&store, "proof", "test", "fail", 60).unwrap();
    let (state, reason) = get_health(&store, "label").unwrap();
    assert_eq!(state, HealthState::CONTRADICTED);
    assert_eq!(
        get_all_health(&store).unwrap(),
        vec![("proof".to_string(), state, reason)]
    );
    let conn = db::Connection::open(health_db_path(&store.root)).unwrap();
    let cached_id: String = conn
        .query_row("SELECT claim_id FROM health_cache", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cached_id, "proof");
}
