//! Integration tests for LCM (Lossless Context Management) and Map operators.

use decapod::core::store::{Store, StoreKind};
use decapod::plugins::lcm::{
    ingest, initialize_lcm_db, list_originals, rebuild_index_from_ledger, show_original,
    show_summary, summarize, validate_ledger_integrity,
};
use decapod::plugins::map_ops::{map_agentic, map_llm, read_map_events};
use std::fs;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, Store) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_lcm_db(&root).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root,
    };
    (tmp, store)
}

// ---------------------------------------------------------------------------
// LCM tests
// ---------------------------------------------------------------------------

#[test]
fn test_lcm_ingest_produces_correct_content_hash() {
    let (_tmp, store) = test_store();
    let content = "The quick brown fox jumps over the lazy dog";
    let result = ingest(&store, content, "message", "test-agent", None, None).unwrap();
    let content_hash = result["content_hash"].as_str().unwrap();

    // Verify SHA256
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let expected = format!("{:x}", hasher.finalize());
    assert_eq!(content_hash, expected);
}

#[test]
fn test_lcm_ingest_is_append_only() {
    let (_tmp, store) = test_store();

    ingest(&store, "first", "message", "agent", None, None).unwrap();
    let ledger_path = store.root.join("lcm.events.jsonl");
    let len1 = fs::read_to_string(&ledger_path).unwrap().lines().count();
    assert_eq!(len1, 1);

    ingest(&store, "second", "event", "agent", None, None).unwrap();
    let len2 = fs::read_to_string(&ledger_path).unwrap().lines().count();
    assert_eq!(len2, 2);

    ingest(&store, "third", "artifact", "agent", None, None).unwrap();
    let len3 = fs::read_to_string(&ledger_path).unwrap().lines().count();
    assert_eq!(len3, 3);

    // Verify ledger only grows
    assert!(len3 > len2);
    assert!(len2 > len1);
}

#[test]
fn test_lcm_summarize_is_deterministic() {
    let (_tmp, store) = test_store();

    ingest(&store, "alpha", "message", "agent", None, None).unwrap();
    ingest(&store, "beta", "message", "agent", None, None).unwrap();
    ingest(&store, "gamma", "event", "agent", None, None).unwrap();

    let result1 = summarize(&store, "all").unwrap();
    let hash1 = result1["summary_hash"].as_str().unwrap().to_string();

    // Summarize again — same originals should produce same hash
    let result2 = summarize(&store, "all").unwrap();
    let hash2 = result2["summary_hash"].as_str().unwrap().to_string();

    assert_eq!(hash1, hash2);
}

#[test]
fn test_lcm_index_rebuildable_from_ledger() {
    let (_tmp, store) = test_store();

    ingest(&store, "one", "message", "agent", Some("s1"), None).unwrap();
    ingest(&store, "two", "event", "agent", Some("s1"), None).unwrap();
    ingest(&store, "three", "artifact", "agent", None, None).unwrap();

    // Get original index state
    let original_list = list_originals(&store, None, None).unwrap();
    assert_eq!(original_list.len(), 3);

    // Delete the DB and rebuild from ledger
    let db_path = store.root.join("lcm.db");
    fs::remove_file(&db_path).unwrap();

    // Reinitialize DB (creates empty tables)
    initialize_lcm_db(&store.root).unwrap();

    // Rebuild from ledger
    let count = rebuild_index_from_ledger(&store).unwrap();
    assert_eq!(count, 3);

    // Verify rebuilt index matches original
    let rebuilt_list = list_originals(&store, None, None).unwrap();
    assert_eq!(rebuilt_list.len(), 3);

    // Verify each entry hash matches
    for orig in &original_list {
        let found = rebuilt_list
            .iter()
            .find(|r| r.content_hash == orig.content_hash);
        assert!(
            found.is_some(),
            "Missing content_hash {} after rebuild",
            orig.content_hash
        );
    }
}

#[test]
fn test_lcm_show_retrieves_original() {
    let (_tmp, store) = test_store();
    let content = "find this content";
    let result = ingest(&store, content, "artifact", "agent", None, None).unwrap();
    let hash = result["content_hash"].as_str().unwrap();

    let event = show_original(&store, hash).unwrap().unwrap();
    assert_eq!(event.content, content);
    assert_eq!(event.kind, "artifact");
}

#[test]
fn test_lcm_summary_show() {
    let (_tmp, store) = test_store();
    ingest(&store, "data1", "message", "agent", None, None).unwrap();
    ingest(&store, "data2", "message", "agent", None, None).unwrap();

    let summary_result = summarize(&store, "all").unwrap();
    let summary_hash = summary_result["summary_hash"].as_str().unwrap();

    let entry = show_summary(&store, Some(summary_hash)).unwrap().unwrap();
    assert_eq!(entry.summary_hash, summary_hash);
    assert_eq!(entry.original_hashes.len(), 2);
    assert_eq!(entry.scope, "all");
}

// ---------------------------------------------------------------------------
// Map operator tests
// ---------------------------------------------------------------------------

#[test]
fn test_map_llm_rejects_empty_items() {
    let (_tmp, store) = test_store();
    let result = map_llm(&store, "[]", "prompt", "{}", "agent");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must not be empty"), "Error was: {}", err);
}

#[test]
fn test_map_agentic_rejects_empty_retain() {
    let (_tmp, store) = test_store();
    let result = map_agentic(&store, "[\"item\"]", "do it", "", "agent");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("scope-reduction"), "Error was: {}", err);

    // Also reject whitespace-only retain
    let result2 = map_agentic(&store, "[\"item\"]", "do it", "   ", "agent");
    assert!(result2.is_err());
}

#[test]
fn test_map_agentic_logs_delegation() {
    let (_tmp, store) = test_store();
    let result = map_agentic(
        &store,
        "[\"task1\", \"task2\", \"task3\"]",
        "process each task",
        "orchestration and error handling",
        "test-agent",
    )
    .unwrap();

    assert_eq!(result["item_count"].as_u64().unwrap(), 3);
    assert_eq!(
        result["retain"].as_str().unwrap(),
        "orchestration and error handling"
    );

    // Verify audit trail
    let events = read_map_events(&store.root).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].op, "map.agentic");
    assert_eq!(events[0].item_count, 3);
    assert_eq!(
        events[0].retain.as_deref().unwrap(),
        "orchestration and error handling"
    );
}

#[test]
fn test_map_llm_processes_items() {
    let (_tmp, store) = test_store();
    let result = map_llm(
        &store,
        "[\"a\", \"b\", \"c\"]",
        "summarize: {{item}}",
        "{\"type\": \"object\"}",
        "agent",
    )
    .unwrap();

    assert_eq!(result["item_count"].as_u64().unwrap(), 3);
    assert!(result["result_hash"].as_str().is_some());
    assert!(result["prompt_hash"].as_str().is_some());
    assert!(result["schema_hash"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// Immutability gate tests
// ---------------------------------------------------------------------------

#[test]
fn test_lcm_immutability_gate_catches_tampered_hash() {
    let (_tmp, store) = test_store();
    ingest(&store, "authentic content", "message", "agent", None, None).unwrap();
    ingest(&store, "more content", "event", "agent", None, None).unwrap();

    // Tamper with a content hash in the ledger
    let ledger_path = store.root.join("lcm.events.jsonl");
    let contents = fs::read_to_string(&ledger_path).unwrap();
    let tampered = contents.replacen("authentic content", "tampered content", 1);
    fs::write(&ledger_path, &tampered).unwrap();

    let failures = validate_ledger_integrity(&store.root).unwrap();
    assert!(!failures.is_empty(), "Should detect tampered content hash");
    assert!(
        failures[0].contains("content_hash mismatch"),
        "Failure message should mention hash mismatch: {}",
        failures[0]
    );
}

#[test]
fn test_lcm_immutability_gate_passes_valid() {
    let (_tmp, store) = test_store();
    ingest(&store, "content a", "message", "agent", None, None).unwrap();
    ingest(&store, "content b", "event", "agent", None, None).unwrap();

    let failures = validate_ledger_integrity(&store.root).unwrap();
    assert!(
        failures.is_empty(),
        "Valid ledger should pass: {:?}",
        failures
    );
}

#[test]
fn test_lcm_ingest_rejects_invalid_kind() {
    let (_tmp, store) = test_store();
    let result = ingest(&store, "test", "invalid_kind", "agent", None, None);
    assert!(result.is_err());
}
