// Moved from src/decapod/plugins/lcm.rs
use super::*;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, Store) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_lcm_db(&root).unwrap();
    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root,
    };
    (tmp, store)
}

#[test]
fn test_ingest_produces_correct_hash() {
    let (_tmp, store) = test_store();
    let content = "Hello, world!";
    let result = ingest(&store, content, "message", "test-agent", None, None).unwrap();
    let expected_hash = sha256_hex(content.as_bytes());
    assert_eq!(result["content_hash"].as_str().unwrap(), expected_hash);
}

#[test]
fn test_ingest_rejects_invalid_kind() {
    let (_tmp, store) = test_store();
    let result = ingest(&store, "test", "bogus", "agent", None, None);
    assert!(result.is_err());
}

#[test]
fn test_list_returns_ingested() {
    let (_tmp, store) = test_store();
    ingest(&store, "alpha", "message", "agent", None, None).unwrap();
    ingest(&store, "beta", "event", "agent", None, None).unwrap();

    let all = list_originals(&store, None, None).unwrap();
    assert_eq!(all.len(), 2);

    let msgs = list_originals(&store, Some("message"), None).unwrap();
    assert_eq!(msgs.len(), 1);
}

#[test]
fn test_show_original_found() {
    let (_tmp, store) = test_store();
    let result = ingest(&store, "find me", "artifact", "agent", None, None).unwrap();
    let hash = result["content_hash"].as_str().unwrap();
    let event = show_original(&store, hash).unwrap().unwrap();
    assert_eq!(event.content, "find me");
}

#[test]
fn test_validate_catches_tamper() {
    let (_tmp, store) = test_store();
    ingest(&store, "good content", "message", "agent", None, None).unwrap();

    // Tamper with the canonical append-only event table.
    let conn = rusqlite::Connection::open(lcm_db_path(&store.root)).unwrap();
    conn.execute(
        "UPDATE lcm_events SET payload = replace(payload, ?1, ?2)",
        ["good content", "bad content"],
    )
    .unwrap();

    let failures = validate_ledger_integrity(&store.root).unwrap();
    assert!(!failures.is_empty());
}
