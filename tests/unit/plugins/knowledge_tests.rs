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
    std::fs::write(
        temp.path().join(".decapod/config.toml"),
        toml::to_string_pretty(&crate::cli::DecapodProjectConfig::default()).unwrap(),
    )
    .unwrap();
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

#[test]
fn equally_authoritative_legacy_entries_without_merge_key_surface_contradiction() {
    let (temp, store) = fixture();
    let mut first = add("K_1", "Prefer synchronous WAL flush for durability");
    first.title = "Storage sync mode";
    first.merge_key = None;
    let mut second = add("K_2", "Prefer deferred background flush for latency");
    second.title = "Storage sync mode";
    second.merge_key = None;
    add_scoped_knowledge(&store, first, "root").unwrap();
    add_scoped_knowledge(&store, second, "repo").unwrap();

    let ctx = resolve_context(&store, temp.path(), Some("flush"), None, &[], None).unwrap();
    assert_eq!(ctx.status, "unresolved_conflict");
    assert_eq!(
        ctx.conflicts,
        vec![vec!["K_1".to_string(), "K_2".to_string()]]
    );
    assert!(ctx.records.is_empty());
    assert_eq!(ctx.excluded.get("unresolved_conflict"), Some(&2));
}

#[test]
fn explicit_supersede_knowledge_links_correction_and_clears_contradiction() {
    let (temp, store) = fixture();
    let mut old = add("K_OLD_SYNC", "Prefer deferred background flush");
    old.title = "Storage sync mode";
    old.merge_key = None;
    add_scoped_knowledge(&store, old, "root").unwrap();

    let mut fix = add("K_NEW_SYNC", "Prefer synchronous WAL flush");
    fix.title = "Storage sync mode";
    fix.merge_key = None;

    assert!(supersede_knowledge(&store, fix.clone(), "root", "K_MISSING").is_err());
    assert!(supersede_knowledge(&store, fix.clone(), "root", "K_NEW_SYNC").is_err());

    let res =
        add_attested_superseding_knowledge(&store, temp.path(), fix, "root", "K_OLD_SYNC").unwrap();
    assert_eq!(res.action, "superseded");
    assert_eq!(res.superseded_ids, vec!["K_OLD_SYNC".to_string()]);
    assert_eq!(
        get_knowledge(&store, "K_OLD_SYNC").unwrap().unwrap().status,
        "superseded"
    );

    let ctx = resolve_context(&store, temp.path(), Some("flush"), None, &[], None).unwrap();
    assert_eq!(ctx.status, "advisory_knowledge_selected");
    assert!(ctx.conflicts.is_empty());
    assert_eq!(ctx.records.len(), 1);
    assert_eq!(ctx.records[0].id, "K_NEW_SYNC");
    assert_eq!(ctx.records[0].supersedes_id.as_deref(), Some("K_OLD_SYNC"));
}

#[test]
fn negative_retrieval_bias_excludes_repeated_hurt_and_rejects_plaintext_non_loopback_url() {
    let (temp, store) = fixture();
    let mut insecure = add("K_HTTP", "Plain HTTP reference");
    insecure.provenance = "url:http://example.invalid/doc";
    assert!(add_knowledge(&store, insecure).is_err());

    let mut helpful = add("K_HELPFUL", "Helpful WAL guidance");
    helpful.title = "WAL guidance";
    helpful.merge_key = None;
    let mut harmful = add("K_HARMFUL", "Harmful WAL guidance");
    harmful.title = "Legacy WAL shortcut";
    harmful.merge_key = None;
    add_knowledge(&store, helpful).unwrap();
    add_knowledge(&store, harmful).unwrap();

    log_retrieval_feedback(
        &store,
        "WAL",
        &["K_HARMFUL".to_string()],
        &["K_HARMFUL".to_string()],
        "hurt",
        "manual_feedback",
    )
    .unwrap();
    log_retrieval_feedback(
        &store,
        "WAL",
        &["K_HARMFUL".to_string()],
        &["K_HARMFUL".to_string()],
        "hurt",
        "manual_feedback",
    )
    .unwrap();
    log_retrieval_feedback(
        &store,
        "WAL",
        &["K_HELPFUL".to_string()],
        &["K_HELPFUL".to_string()],
        "helped",
        "manual_feedback",
    )
    .unwrap();

    let ctx = resolve_context(&store, temp.path(), Some("WAL"), None, &[], None).unwrap();
    assert_eq!(ctx.records.len(), 1);
    assert_eq!(ctx.records[0].id, "K_HELPFUL");
    assert_eq!(
        ctx.records[0].authority,
        "advisory_below_current_intent_and_policy"
    );
    assert_eq!(ctx.excluded.get("negative_retrieval_bias"), Some(&1));
}
