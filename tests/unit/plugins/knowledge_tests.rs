use super::*;
use crate::core::{db, store::StoreKind};

fn fixture() -> (tempfile::TempDir, Store) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: temp.path().join(".decapod/data"),
    };
    db::initialize_knowledge_db(&store.root).unwrap();
    crate::plugins::federation::initialize_federation_db(&store.root).unwrap();
    // Isolated fixture writer; this label does not authenticate a human decision.
    DbBroker::new(&store.root).with_conn(&knowledge_db_path(&store.root), "decapod", None,
        "policy.init", |conn| {
        conn.execute("INSERT INTO agents(agent_id, trust_level, updated_at) VALUES('fixture', 'verified', '1Z')", [])?;
        Ok(())
    }).unwrap();
    (temp, store)
}

fn packet(store: &Store, root: &Path) -> KnowledgeContext {
    resolve_context(store, root, Some("design"), Some("fixture-task"), &[], None).unwrap()
}

#[test]
fn fresh_session_projects_the_correction_and_preserves_rejected_history() {
    let (temp, store) = fixture();
    add_knowledge(&store, add("B", &"rejected design B ".repeat(20))).unwrap();
    add_knowledge(&store, add("A", "current design A")).unwrap();
    let fresh = Store {
        kind: store.kind.clone(),
        root: store.root.clone(),
    };
    let context = packet(&fresh, temp.path());
    assert_eq!(context.records.len(), 1);
    assert_eq!(context.records[0].excerpt, "current design A");
    assert!(
        context.records[0]
            .authority
            .starts_with("advisory_below_current_intent")
    );
    let old = get_knowledge(&fresh, "B").unwrap().unwrap();
    assert_eq!(old.status, "superseded");
    assert_eq!(old.content.matches("design B").count(), 20);
    assert_eq!(packet(&fresh, temp.path()), context);
}

#[test]
fn merge_rejection_changes_the_next_packet_and_records_the_prior_body() {
    let (temp, store) = fixture();
    add_knowledge(&store, add("B", "previous design B")).unwrap();
    let mut correction = add("correction", "rejected design B");
    correction.conflict_policy = KnowledgeConflictPolicy::Merge;
    correction.status = "rejected";
    add_knowledge(&store, correction).unwrap();
    assert!(packet(&store, temp.path()).records.is_empty());
    assert_eq!(
        get_knowledge(&store, "B").unwrap().unwrap().status,
        "rejected"
    );
    let history = events::query_serialized_lines(&store.root, events::KNOWLEDGE).unwrap();
    assert!(history.contains("knowledge.before_correction"));
    assert!(history.contains("previous design B"));
    let audit = knowledge_history(&store, "B").unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0]["entry"]["content"], "previous design B");
}

#[test]
fn private_unrelated_and_expired_records_never_enter_the_packet() {
    let (temp, store) = fixture();
    for (id, scope) in [
        ("private", "private"),
        ("other", "task:other-task"),
        ("path", "path:unrelated"),
    ] {
        add_scoped_knowledge(&store, add(id, "private design B"), scope).unwrap();
    }
    let mut expired = add("expired", "expired design B");
    expired.expires_ts = Some("1Z");
    add_knowledge(&store, expired).unwrap();
    add_scoped_knowledge(&store, add("A", "task design A"), "task:fixture-task").unwrap();
    let context = packet(&store, temp.path());
    assert_eq!(context.records.len(), 1);
    assert_eq!(context.records[0].excerpt, "task design A");
    assert_eq!(context.excluded.get("expired_or_invalid_expiry"), Some(&1));
    assert!(
        !serde_json::to_string(&context)
            .unwrap()
            .contains("private design")
    );
}

#[test]
fn equally_applicable_typed_decisions_surface_conflict_without_their_bodies() {
    let (temp, store) = fixture();
    let mut ids = Vec::new();
    for body in ["design A", "design B design B design B"] {
        let node = crate::plugins::federation::add_node(
            &store,
            "design choice",
            "decision",
            "notable",
            "human_confirmed",
            body,
            "cmd:fixture",
            "accepted",
            "root",
            None,
            "fixture",
        )
        .unwrap();
        ids.push(format!("federation:{}", node.id));
    }
    let context = packet(&store, temp.path());
    ids.sort();
    assert_eq!(context.status, "unresolved_conflict");
    assert_eq!(context.conflicts, vec![ids]);
    assert!(context.records.is_empty());
    assert!(
        !serde_json::to_string(&context)
            .unwrap()
            .contains("design B")
    );
}

#[test]
fn output_budget_is_utf8_safe_and_declared_confidence_does_not_promote_authority() {
    let (temp, store) = fixture();
    for i in 0..12 {
        let id = format!("record-{i:02}");
        let content = "design éλ ".repeat(200);
        let mut entry = add(&id, &content);
        entry.merge_key = None;
        add_knowledge(&store, entry).unwrap();
    }
    let context = packet(&store, temp.path());
    assert_eq!(context.records.len(), 8);
    assert!(context.content_bytes <= 4096);
    assert!(
        context
            .records
            .iter()
            .all(|record| record.excerpt.len() <= 512)
    );
    assert!(serde_json::to_vec(&context).unwrap().len() < 24 * 1024);
}

#[test]
fn credentials_are_rejected_and_unversioned_sources_remain_explicit() {
    let (temp, store) = fixture();
    let secret = format!("api_key={}", "x".repeat(32));
    assert!(add_knowledge(&store, add("secret", &secret)).is_err());
    let mut source = add("source", "design advice");
    source.provenance = "file:missing.md#L1";
    add_knowledge(&store, source).unwrap();
    let context = packet(&store, temp.path());
    assert_eq!(context.records[0].freshness, "source_unavailable");
    assert_eq!(
        context.records[0].provenance_class,
        "recorded_source_unverified"
    );
}

#[test]
fn changed_sources_invalidate_identity_without_inventing_historical_freshness() {
    let (temp, store) = fixture();
    let source_path = temp.path().join("decision.md");
    std::fs::write(&source_path, "design A").unwrap();
    let mut source = add("source", "design advice");
    source.provenance = "file:decision.md";
    add_knowledge(&store, source).unwrap();
    let first = packet(&store, temp.path());
    std::fs::write(&source_path, "current correction").unwrap();
    let second = packet(&store, temp.path());
    assert_ne!(first.source_hash, second.source_hash);
    assert_eq!(second.records[0].freshness, "not_captured_at_write");
    assert_ne!(
        first.records[0].source_observation,
        second.records[0].source_observation
    );
}

fn add<'a>(id: &'a str, content: &'a str) -> AddKnowledgeParams<'a> {
    AddKnowledgeParams {
        id,
        title: "fixture decision",
        content,
        provenance: "cmd:fixture",
        claim_id: None,
        merge_key: Some("fixture-choice"),
        conflict_policy: KnowledgeConflictPolicy::Supersede,
        status: "active",
        ttl_policy: "ephemeral",
        expires_ts: None,
    }
}

#[test]
fn expired_entries_are_not_live_guidance_without_a_decay_job() {
    let (_temp, store) = fixture();
    let mut expired = add("B", "rejected design B");
    expired.expires_ts = Some("1Z");
    add_knowledge(&store, expired).unwrap();
    let result = search_knowledge(
        &store,
        "design",
        SearchOptions {
            as_of: None,
            window_days: None,
            rank: "relevance",
        },
    )
    .unwrap();
    assert!(
        result.is_empty(),
        "expired knowledge resurfaced without running manual decay"
    );
}

#[test]
fn failed_supersession_does_not_destroy_the_previous_active_record() {
    let (_temp, store) = fixture();
    add_knowledge(&store, add("B", "original B")).unwrap();
    assert!(add_knowledge(&store, add("B", "replacement with colliding id")).is_err());
    let result = search_knowledge(
        &store,
        "original",
        SearchOptions {
            as_of: None,
            window_days: None,
            rank: "relevance",
        },
    )
    .unwrap();
    assert_eq!(
        result.len(),
        1,
        "failed replacement changed committed active state"
    );
    assert_eq!(result[0].id, "B");
}

#[test]
fn bundle_import_rejects_corrected_knowledge_and_legacy_omission() {
    use crate::core::{context_bundle, context_capsule};
    let (temp, store) = fixture();
    // A real repository is required for repository-signal and config identity.
    let git = std::process::Command::new("git")
        .args(["init", "-b", "master"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(git.status.success());
    add_knowledge(&store, add("B", "previous design B")).unwrap();
    let capsule = context_capsule::query_embedded_capsule(
        temp.path(),
        "design",
        "core",
        Some("fixture-task"),
        None,
        2,
    )
    .unwrap();
    let bundle = context_bundle::build_bundle(temp.path(), capsule, None, vec![], vec![]).unwrap();
    context_bundle::validate_bundle(temp.path(), &bundle).unwrap();
    let mut legacy = bundle.clone();
    legacy.capsule.knowledge = None;
    legacy.capsule = legacy.capsule.with_recomputed_hash().unwrap();
    legacy = legacy.with_recomputed_hash().unwrap();
    assert!(
        context_bundle::validate_bundle(temp.path(), &legacy)
            .unwrap_err()
            .to_string()
            .contains("STALE_KNOWLEDGE")
    );
    add_knowledge(&store, add("A", "current design A")).unwrap();
    assert!(
        context_bundle::validate_bundle(temp.path(), &bundle)
            .unwrap_err()
            .to_string()
            .contains("STALE_KNOWLEDGE")
    );
    assert_eq!(
        get_knowledge(&store, "B").unwrap().unwrap().status,
        "superseded"
    );
}
