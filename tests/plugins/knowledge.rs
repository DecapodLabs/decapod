use decapod::core::db;
use decapod::core::store::{Store, StoreKind};
use decapod::plugins::knowledge::{
    AddKnowledgeParams, SearchOptions, add_knowledge, decay_knowledge, log_retrieval_feedback,
    parse_conflict_policy, search_knowledge,
};
use tempfile::tempdir;

#[test]
fn test_knowledge_merge_reject_blocks_duplicate_active() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };
    db::initialize_knowledge_db(&store.root).unwrap();

    add_knowledge(
        &store,
        AddKnowledgeParams {
            id: "K_1",
            title: "Title 1",
            content: "Body",
            provenance: "file:README.md#L1",
            claim_id: None,
            merge_key: Some("auth-flow"),
            conflict_policy: parse_conflict_policy("reject").unwrap(),
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
    )
    .unwrap();

    let err = add_knowledge(
        &store,
        AddKnowledgeParams {
            id: "K_2",
            title: "Title 2",
            content: "Body 2",
            provenance: "file:README.md#L2",
            claim_id: None,
            merge_key: Some("auth-flow"),
            conflict_policy: parse_conflict_policy("reject").unwrap(),
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
    )
    .unwrap_err();

    assert!(format!("{}", err).contains("merge_key conflict"));
}

#[test]
fn test_knowledge_search_respects_as_of_cutoff() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };
    db::initialize_knowledge_db(&store.root).unwrap();

    add_knowledge(
        &store,
        AddKnowledgeParams {
            id: "K_10",
            title: "A",
            content: "same-query",
            provenance: "file:README.md#L10",
            claim_id: None,
            merge_key: None,
            conflict_policy: parse_conflict_policy("reject").unwrap(),
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
    )
    .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    add_knowledge(
        &store,
        AddKnowledgeParams {
            id: "K_11",
            title: "B",
            content: "same-query",
            provenance: "file:README.md#L11",
            claim_id: None,
            merge_key: None,
            conflict_policy: parse_conflict_policy("reject").unwrap(),
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
    )
    .unwrap();

    let first_created = search_knowledge(
        &store,
        "same-query",
        SearchOptions {
            as_of: None,
            window_days: None,
            rank: "recency_desc",
        },
    )
    .unwrap()
    .last()
    .unwrap()
    .created_at
    .clone();

    let results = search_knowledge(
        &store,
        "same-query",
        SearchOptions {
            as_of: Some(&first_created),
            window_days: None,
            rank: "recency_desc",
        },
    )
    .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "K_10");
}

#[test]
fn test_decay_is_deterministic_for_same_as_of() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };
    db::initialize_knowledge_db(&store.root).unwrap();

    add_knowledge(
        &store,
        AddKnowledgeParams {
            id: "K_20",
            title: "expiring",
            content: "foo",
            provenance: "file:README.md#L20",
            claim_id: None,
            merge_key: None,
            conflict_policy: parse_conflict_policy("reject").unwrap(),
            status: "active",
            ttl_policy: "ephemeral",
            expires_ts: Some("1Z"),
        },
    )
    .unwrap();

    let r1 = decay_knowledge(&store, "default", Some("1000Z"), true).unwrap();
    let r2 = decay_knowledge(&store, "default", Some("1000Z"), true).unwrap();
    assert_eq!(r1.stale_ids, r2.stale_ids);
}

#[test]
fn test_retrieval_feedback_log_writes_event() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };

    let result = log_retrieval_feedback(
        &store,
        "codex",
        "query",
        &["K_1".to_string(), "K_2".to_string()],
        &["K_1".to_string()],
        "helped",
    )
    .unwrap();

    let content =
        std::fs::read_to_string(store.root.join("memory.retrieval_events.jsonl")).unwrap();
    assert!(content.contains(&result.event_id));
    assert!(content.contains("\"outcome\":\"helped\""));
}
