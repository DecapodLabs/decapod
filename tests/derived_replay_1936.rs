//! Wave 3 Hardening Tests (#1936, #1630, #657):
//! - #1936: Deterministic derived projection replay, bounded cursor resumption,
//!   gap/out-of-order/mid-read-compaction refusal, torn/corrupt projection
//!   recovery, and non-custodial stale-view invariants.
//! - #1630: Skewed-history relational/event compaction, concurrent reader/writer +
//!   compactor safety, pre/post-activation interruption, bounded WAL/free-space
//!   & index amplification, and logical digest parity across restore/replay.
//! - #657: Full rebuild vs. incremental knowledge convergence across
//!   added/merged/superseded/revoked/duplicated records, >2048 event pagination,
//!   negative controls (source removal, malformed provenance, interrupted update,
//!   scope bleed), and strict procedural-promotion firewall.

use decapod::core::db::{self, Connection, params};
use decapod::core::events;
use decapod::core::schemas;
use decapod::core::storage_compaction;
use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{
    self, ClaimMode, TaskJournalVerifyBudget, TaskJournalVerifyCursor, TodoCommand,
};
use decapod::core::workunit;
use decapod::plugins::federation;
use decapod::plugins::knowledge::{
    self, AddKnowledgeParams, KnowledgeConflictPolicy, KnowledgePromotionEventInput, SearchOptions,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

fn init_store_fixture() -> (TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_root = tmp.path().to_path_buf();
    let data = repo_root.join(".decapod/data");
    fs::create_dir_all(&data).expect("create data dir");
    todo::initialize_todo_db(&data).expect("init todo db");
    db::initialize_knowledge_db(&data).expect("init knowledge db");
    federation::initialize_federation_db(&data).expect("init federation db");
    (tmp, repo_root, data)
}

fn add_sample_task(data: &Path, dir: &Path, title: &str, desc: &str, scope: &str) -> String {
    let added = todo::add_task(
        data,
        &TodoCommand::Add {
            title: title.to_string(),
            description: desc.to_string(),
            priority: "high".to_string(),
            tags: "replay,hardening".to_string(),
            owner: String::new(),
            due: None,
            r#ref: "issue:1936".to_string(),
            scope: scope.to_string(),
            dir: Some(dir.to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("add_task");
    added["id"].as_str().expect("task id").to_string()
}

fn measure_sqlite_indexes(conn: &Connection) -> usize {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='index'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("index count") as usize
}

// ============================================================================
// #1936 — Derived Projection Replay, Resumable Cursors & Recovery Boundaries
// ============================================================================

#[test]
fn replay_1936_clean_snapshot_plus_suffix_matches_incremental_digest() {
    let (_tmp, repo, data) = init_store_fixture();
    let long_desc = "Deterministic replay immutable component payload. ".repeat(6);
    let t1 = add_sample_task(&data, &repo, "Task Alpha", &long_desc, "repo");
    let t2 = add_sample_task(&data, &repo, "Task Beta", "short desc", "repo");

    for rev in 1..=5 {
        todo::edit_task(
            &data,
            &t1,
            Some(&format!("Task Alpha rev {rev}")),
            None,
            None,
            None,
        )
        .expect("edit t1");
    }
    todo::claim_task(&data, &t2, "agent-replay-1", ClaimMode::Exclusive).expect("claim t2");
    todo::edit_task(
        &data,
        &t2,
        Some("Task Beta claimed"),
        Some(&long_desc),
        None,
        None,
    )
    .expect("edit t2");

    let full = todo::task_journal_verify(&data).expect("full verify");
    assert_eq!(full["status"], "ok");
    assert_eq!(full["complete"], true);
    let expected_digest = full["current_digest"]
        .as_str()
        .expect("current_digest")
        .to_string();

    // Incremental paged replay (budgeted cursor serialized across steps) must match identically.
    let db_path = todo::todo_db_path(&data);
    let conn = db::db_connect_for_validate(&db_path.to_string_lossy()).expect("open ro");
    let budget = TaskJournalVerifyBudget {
        page_size: 2,
        max_checkpoint_rows: Some(2),
        max_task_rows: Some(1),
    };
    let mut cursor: Option<TaskJournalVerifyCursor> = None;
    let mut partials = 0usize;
    let final_incremental = loop {
        let step = todo::verify_task_journal_with_budget(&conn, &budget, cursor.as_ref())
            .expect("step verify");
        if step["complete"] == true {
            break step;
        }
        partials += 1;
        assert_eq!(step["status"], "partial");
        let parsed: TaskJournalVerifyCursor =
            serde_json::from_value(step["continuation_cursor"].clone()).expect("cursor roundtrip");
        cursor = Some(parsed);
    };
    assert!(partials >= 3, "expected multiple partial cursor steps");
    assert_eq!(final_incremental["status"], "ok");
    assert_eq!(final_incremental["current_digest"], expected_digest);

    // Delete derived projection rows for t1 and restore from canonical checkpoints + suffix.
    drop(conn);
    let wconn = Connection::open(&db_path).expect("open rw");
    wconn
        .execute("DELETE FROM tasks WHERE id = ?1", [&t1])
        .expect("delete derived task row");
    drop(wconn);

    let conflict = todo::task_journal_verify(&data).expect("detect missing task");
    assert_eq!(conflict["status"], "conflict");
    let current_conflict_digest = conflict["current_digest"]
        .as_str()
        .expect("conflict digest");

    let restored =
        todo::task_journal_restore(&data, current_conflict_digest).expect("restore projection");
    assert_eq!(restored["status"], "ok");
    assert_eq!(restored["current_digest"], expected_digest);
}

#[test]
fn replay_1936_overlapping_batch_replay_is_idempotent_and_conflict_safe() {
    let (_tmp, repo, data) = init_store_fixture();
    let db_path = events::canonical_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");
    events::ensure_tables(&conn).expect("ensure events");

    let ev1 = json!({
        "event_id": "01M85REPLAY000000000000001",
        "ts": "1795400001Z",
        "event_type": "knowledge.note",
        "actor": "agent-a",
        "note": "batch-1"
    });
    let ev2 = json!({
        "event_id": "01M85REPLAY000000000000002",
        "ts": "1795400002Z",
        "event_type": "knowledge.note",
        "actor": "agent-a",
        "note": "batch-2"
    });

    let s1 = events::append_on_conn(&conn, events::KNOWLEDGE, &ev1).expect("append ev1");
    let s2 = events::append_on_conn(&conn, events::KNOWLEDGE, &ev2).expect("append ev2");

    // Overlapping replay of [ev1, ev2] returns original sequences without duplicating rows.
    assert_eq!(
        events::append_on_conn(&conn, events::KNOWLEDGE, &ev1).expect("replay ev1"),
        s1
    );
    assert_eq!(
        events::append_on_conn(&conn, events::KNOWLEDGE, &ev2).expect("replay ev2"),
        s2
    );

    let mut conflicting = ev1.clone();
    conflicting["note"] = json!("mutated-payload");
    let err = events::append_on_conn(&conn, events::KNOWLEDGE, &conflicting)
        .expect_err("changed payload with same event_id must conflict");
    assert!(err.to_string().contains("EVENT_ID_CONFLICT"), "{err}");

    // Workunit overlapping operation_id replay vs changed manifest conflict.
    let task_id = add_sample_task(&data, &repo, "Workunit Idempotency", "desc", "repo");
    workunit::init_workunit(&repo, &task_id, "intent:idemp").expect("init workunit");
    let mut manifest = workunit::load_workunit(&repo, &task_id).expect("load workunit");
    manifest.state_refs.push("state:step-1".to_string());
    workunit::write_workunit_checked(&repo, &manifest, "op-replay-1", 1).expect("first write");
    workunit::write_workunit_checked(&repo, &manifest, "op-replay-1", 1)
        .expect("idempotent overlapping replay");
    manifest.state_refs.push("state:tampered".to_string());
    let wu_err = workunit::write_workunit_checked(&repo, &manifest, "op-replay-1", 1)
        .expect_err("changed manifest under same operation_id must fail");
    assert!(
        wu_err.to_string().contains("WORKUNIT_OPERATION_CONFLICT"),
        "{wu_err}"
    );
}

#[test]
fn replay_1936_gaps_out_of_order_mid_read_compaction_and_exempt_delegation() {
    let (_tmp, repo, data) = init_store_fixture();
    let t1 = add_sample_task(&data, &repo, "Gap Subject", "initial", "repo");
    todo::edit_task(&data, &t1, Some("Gap Subject v2"), None, None, None).expect("edit t1");

    let db_path = todo::todo_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");

    // 1) Non-mutating delegation audit event after checkpoint must be uniformly exempt across
    //    both check_complete_suffix_for_task and verify_task_journal_with_budget.
    events::append_on_conn(
        &conn,
        events::TODO,
        &json!({
            "ts": "1795400010Z",
            "event_id": "delegation-audit-01",
            "event_type": "task.delegation.policy",
            "status": "success",
            "task_id": t1,
            "actor": "agent-a",
            "payload": {"decision": "allow"}
        }),
    )
    .expect("append delegation policy");
    let verified_with_delegation =
        todo::task_journal_verify(&data).expect("delegation policy is journal-exempt");
    assert_eq!(verified_with_delegation["status"], "ok");

    // 2) Mid-read mutation/compaction invalidates cursor snapshot rather than returning mixed state.
    let budget = TaskJournalVerifyBudget {
        page_size: 1,
        max_checkpoint_rows: Some(1),
        max_task_rows: Some(1),
    };
    let step1 = todo::verify_task_journal_with_budget(&conn, &budget, None).expect("step1");
    assert_eq!(step1["status"], "partial");
    let cursor: TaskJournalVerifyCursor =
        serde_json::from_value(step1["continuation_cursor"].clone()).expect("cursor");

    // Mutate datastore before resuming cursor.
    let _t2 = add_sample_task(&data, &repo, "Concurrent Task", "added mid-read", "repo");
    let snap_err = todo::verify_task_journal_with_budget(&conn, &budget, Some(&cursor))
        .expect_err("mid-read mutation must invalidate cursor");
    assert!(
        snap_err
            .to_string()
            .contains("TASK_JOURNAL_SNAPSHOT_CHANGED"),
        "{snap_err}"
    );

    // 3) Uncheckpointed domain-event suffix must fail closed (never invent prior state).
    events::append_on_conn(
        &conn,
        events::TODO,
        &json!({
            "ts": "1795400020Z",
            "event_id": "uncheckpointed-domain-01",
            "event_type": "task.update",
            "status": "success",
            "task_id": t1,
            "actor": "agent-a",
            "payload": {"title": "uncheckpointed"}
        }),
    )
    .expect("append uncheckpointed domain event");
    let suffix_err = todo::task_journal_verify(&data).expect_err("uncheckpointed suffix fails");
    assert!(
        suffix_err
            .to_string()
            .contains("TASK_JOURNAL_INCOMPLETE_HISTORY"),
        "{suffix_err}"
    );

    // 4) Corrupt/out-of-order predecessor in checkpoint chain fails closed.
    let (_tmp2, repo2, data2) = init_store_fixture();
    let t_bad = add_sample_task(&data2, &repo2, "Order Check", "desc", "repo");
    todo::edit_task(&data2, &t_bad, Some("Order Check v2"), None, None, None).expect("edit");
    let conn2 = Connection::open(todo::todo_db_path(&data2)).expect("open db2");
    let ckpts: Vec<_> = events::query_subject_on_conn(&conn2, events::TODO, "task", &t_bad, 100)
        .expect("query checkpoints")
        .into_iter()
        .filter(|e| e.event_type == "task.journal.checkpoint")
        .collect();
    assert_eq!(ckpts.len(), 2);
    // Swap seq order of baseline (ordinal 1) and transition (ordinal 2).
    let seq0 = ckpts[0].seq;
    let seq1 = ckpts[1].seq;
    conn2
        .execute_batch(&format!(
            "BEGIN IMMEDIATE;
             UPDATE events SET seq = 9999 WHERE event_id = '{}';
             UPDATE events SET seq = {seq0} WHERE event_id = '{}';
             UPDATE events SET seq = {seq1} WHERE event_id = '{}';
             COMMIT;",
            ckpts[0].event_id, ckpts[1].event_id, ckpts[0].event_id
        ))
        .expect("swap checkpoint seq");
    let order_err = todo::task_journal_verify(&data2).expect_err("out-of-order chain must fail");
    assert!(
        order_err.to_string().contains("TASK_JOURNAL_CONFLICT"),
        "{order_err}"
    );
}

#[test]
fn replay_1936_torn_or_corrupt_derived_file_recovery_never_grants_custody() {
    let (_tmp, repo, data) = init_store_fixture();
    let task_id = add_sample_task(&data, &repo, "Torn Recovery", "desc", "repo");
    workunit::init_workunit(&repo, &task_id, "intent:torn").expect("init workunit");
    workunit::add_state_ref(&repo, &task_id, "state:durable-1").expect("add state ref");
    let expected = workunit::load_workunit(&repo, &task_id).expect("load clean workunit");

    let path = workunit::workunit_path(&repo, &task_id).expect("workunit path");
    // Simulate torn/truncated derived file on disk.
    fs::write(&path, b"{\"task_id\":\"torn-partial").expect("write torn file");
    let load_err = workunit::load_workunit(&repo, &task_id).expect_err("torn file must fail");
    assert!(
        load_err.to_string().contains("WORKUNIT_RECOVERY_REQUIRED"),
        "{load_err}"
    );

    let recovered = workunit::recover_workunit(&repo, &task_id).expect("recover from journal");
    assert_eq!(recovered.projection_state, "current");
    assert!(!recovered.resumable, "recovered view must not grant resume");
    assert_eq!(
        recovered.proof_status, "not_proven",
        "recovered view must not grant proof/custody"
    );
    assert_eq!(
        workunit::load_workunit(&repo, &task_id).expect("reload recovered"),
        expected
    );

    // Unrecognized valid JSON in derived file must be preserved and refuse blind overwrite.
    let mut foreign = expected.clone();
    foreign.intent_ref = "intent:foreign-edit".to_string();
    fs::write(&path, serde_json::to_vec(&foreign).unwrap()).expect("write foreign valid json");
    let conflict_err = workunit::recover_workunit(&repo, &task_id)
        .expect_err("unrecognized valid derived file must fail closed");
    assert!(
        conflict_err
            .to_string()
            .contains("WORKUNIT_PROJECTION_CONFLICT"),
        "{conflict_err}"
    );
}

#[test]
fn replay_1936_large_stream_bounded_cursor_and_insufficient_history_refusal() {
    let (_tmp, repo, data) = init_store_fixture();
    let initial_total = events::query(&data, events::KNOWLEDGE, usize::MAX)
        .unwrap()
        .len()
        + events::query(&data, events::BROKER, usize::MAX)
            .unwrap()
            .len()
        + events::query(&data, events::TRACES, usize::MAX)
            .unwrap()
            .len();
    // Write 1,250 canonical events across 3 streams (exceeding QUERY_PAGE_SIZE = 1024 and 256).
    let db_path = events::canonical_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");
    events::ensure_tables(&conn).expect("ensure events");
    conn.execute_batch("BEGIN IMMEDIATE").expect("begin");
    for i in 0..1250usize {
        let stream = match i % 3 {
            0 => events::KNOWLEDGE,
            1 => events::BROKER,
            _ => events::TRACES,
        };
        events::append_on_conn(
            &conn,
            stream,
            &json!({
                "event_id": format!("01M85LARGE{i:016}"),
                "ts": "1795400100Z",
                "event_type": "synthetic.tick",
                "actor": "replay-bench",
                "idx": i,
            }),
        )
        .expect("append synthetic");
    }
    conn.execute_batch("COMMIT").expect("commit");

    let k_events = events::query(&data, events::KNOWLEDGE, usize::MAX).expect("query knowledge");
    let b_events = events::query(&data, events::BROKER, usize::MAX).expect("query broker");
    let t_events = events::query(&data, events::TRACES, usize::MAX).expect("query traces");
    assert_eq!(
        k_events.len() + b_events.len() + t_events.len(),
        initial_total + 1250,
        "no canonical event may be silently dropped across page boundaries"
    );

    // Resumable bounded verify cursor over 40 tasks x 3 checkpoints = 120 checkpoints.
    let mut task_ids = Vec::new();
    for i in 0..40 {
        let id = add_sample_task(&data, &repo, &format!("Bulk {i}"), "bulk desc", "repo");
        todo::edit_task(&data, &id, Some(&format!("Bulk {i} r2")), None, None, None).unwrap();
        todo::edit_task(&data, &id, Some(&format!("Bulk {i} r3")), None, None, None).unwrap();
        task_ids.push(id);
    }
    let expected = todo::task_journal_verify(&data).expect("unbounded verify");
    assert_eq!(expected["status"], "ok");

    let budget = TaskJournalVerifyBudget {
        page_size: 16,
        max_checkpoint_rows: Some(25),
        max_task_rows: Some(10),
    };
    let mut cursor = None;
    let mut steps = 0usize;
    let paged_final = loop {
        let res = todo::verify_task_journal_with_budget(&conn, &budget, cursor.as_ref())
            .expect("paged verify");
        steps += 1;
        if res["complete"] == true {
            break res;
        }
        cursor = Some(
            serde_json::from_value::<TaskJournalVerifyCursor>(res["continuation_cursor"].clone())
                .expect("cursor parse"),
        );
    };
    assert!(steps >= 6);
    assert_eq!(paged_final["current_digest"], expected["current_digest"]);

    // Insufficient history: legacy rebuild refuses to fabricate task projections from incomplete logs.
    let rebuild_report = todo::rebuild_from_events(&data).expect("rebuild report");
    assert_eq!(rebuild_report["task_projection_replayed"], false);
}

// ============================================================================
// #1630 — Skewed Compaction, Concurrency, Crash/Activation & Amplification
// ============================================================================

#[test]
fn compaction_1630_skewed_multi_stream_page_boundary_lossless_and_amplification() {
    let (_tmp, repo, data) = init_store_fixture();
    let large_desc = "Repeated immutable component text for #1630 compaction. ".repeat(8);
    let long_task = add_sample_task(&data, &repo, "Long-lived skewed task", &large_desc, "repo");
    for rev in 1..=20 {
        todo::edit_task(
            &data,
            &long_task,
            Some(&format!("Long-lived skewed task rev {rev}")),
            None,
            None,
            None,
        )
        .expect("edit long task");
    }

    let db_path = events::canonical_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch(schemas::APTITUDE_DB_SCHEMA_AGENT_PROMPTS)
        .expect("agent_prompts");
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name);
         CREATE INDEX IF NOT EXISTS idx_risk_zones_name ON risk_zones(zone_name);
         CREATE INDEX IF NOT EXISTS idx_task_owners_task ON task_owners(task_id);
         CREATE INDEX IF NOT EXISTS idx_task_dependencies_task ON task_dependencies(task_id);",
    )
    .expect("seed obsolete prefix indexes");
    for i in 0..10 {
        conn.execute(
            "INSERT INTO agent_prompts(id, context, prompt_text, priority, active, usage_count, created_at)
             VALUES(?1, 'ctx', 'duplicate prompt text', 1, 1, 1, '1795400000Z')",
            params![format!("prompt-{i}")],
        )
        .expect("insert dup prompt");
    }

    // Seed 300+ events across 3 streams with overlapping seq values (1..=120 per stream)
    // so the 256-row page boundary in compact_historical_events & logical_events_inventory
    // lands squarely inside a tied seq across multiple streams.
    conn.execute_batch("BEGIN IMMEDIATE").expect("begin");
    for i in 0..120usize {
        for stream in [events::BROKER, events::KNOWLEDGE, events::TRACES] {
            let verbose = json!({
                "event_id": format!("01M85SKEW{stream}{i:010}"),
                "ts": "1795400200Z",
                "event_type": "skew.audit",
                "actor": "skew-agent",
                "status": "success",
                "schema_version": "1.1.0",
                "db_id": "decapod.db",
                "actor_id": "skew-agent",
                "request_id": "",
                "caller_agent_id": "",
                "detail": format!("payload-{i}"),
            });
            events::append_on_conn(&conn, stream, &verbose).expect("append skewed event");
        }
    }
    // Insert one unreferenced storage component to verify GC preview & execution.
    conn.execute_batch(schemas::STORAGE_COMPONENTS_SCHEMA)
        .expect("components schema");
    conn.execute(
        "INSERT INTO storage_components(component_kind, codec_version, byte_len, content_sha256, stored_bytes, created_at)
         VALUES('task.description', 0, 12, 'deadbeef00000000000000000000000000000000000000000000000000000000', X'756e7265666572656e636564', '1795400200Z')",
        [],
    )
    .expect("insert orphan component");
    conn.execute_batch("COMMIT").expect("commit");

    let indexes_before = measure_sqlite_indexes(&conn);
    let total_events_before: u64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("total events");
    let journal_before = todo::task_journal_verify(&data).expect("journal before");
    let digest_before = journal_before["current_digest"]
        .as_str()
        .unwrap()
        .to_string();

    // Relational compaction (redundant prefix-index elimination + WITHOUT ROWID on meta/task_tags).
    drop(conn);
    (decapod::core::migration::all_migrations()
        .into_iter()
        .find(|m| m.id == "sqlite.compact_redundant_relational_indexes.v001")
        .expect("index compaction migration")
        .up)(&repo.join(".decapod"))
    .expect("relational index compaction");
    let conn = Connection::open(&db_path).expect("reopen db");
    let indexes_after = measure_sqlite_indexes(&conn);
    assert!(
        indexes_after < indexes_before,
        "prefix-index compaction must reduce index amplification ({indexes_before} -> {indexes_after})"
    );

    // Dry-run preview must detect orphan component & zero mutations.
    let preview =
        storage_compaction::compact_datastore_lossless(&data, true, false).expect("dry run");
    assert_eq!(preview.events_logical_count, total_events_before);
    assert_eq!(preview.components_collected, 1);

    // Execute compaction + explicit page reclamation.
    drop(conn);
    let applied =
        storage_compaction::compact_datastore_lossless(&data, false, true).expect("compact");
    assert!(applied.lossless_verified);
    assert!(applied.reclaim_executed);
    assert_eq!(applied.after.freelist_count, 0);
    assert_eq!(applied.after.reclaimable_bytes, 0);
    assert_eq!(applied.components_collected, 1);
    assert!(
        applied.events_logical_count >= total_events_before,
        "every multi-stream event including tied-seq page boundary rows must be accounted for"
    );

    // Verify logical digest parity across task journal & restore/replay.
    let journal_after = todo::task_journal_verify(&data).expect("journal after");
    assert_eq!(journal_after["status"], "ok");
    assert_eq!(journal_after["current_digest"], digest_before);
}

#[test]
fn compaction_1630_concurrent_reader_writer_and_compactor() {
    let (_tmp, repo, data) = init_store_fixture();
    for i in 0..8 {
        add_sample_task(
            &data,
            &repo,
            &format!("Concurrent Seed {i}"),
            "desc",
            "repo",
        );
    }

    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::new();

    // Writer thread
    {
        let b = Arc::clone(&barrier);
        let d = data.clone();
        let r = repo.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            for i in 0..6 {
                let id = add_sample_task(&d, &r, &format!("Concurrent Write {i}"), "wdesc", "repo");
                todo::edit_task(
                    &d,
                    &id,
                    Some(&format!("Concurrent Write {i} v2")),
                    None,
                    None,
                    None,
                )
                .expect("concurrent edit");
            }
        }));
    }

    // Reader thread (paged journal verifier)
    {
        let b = Arc::clone(&barrier);
        let d = data.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..6 {
                let res = todo::task_journal_verify(&d);
                if let Ok(val) = res {
                    assert_eq!(val["journal_valid"], true);
                }
            }
        }));
    }

    // Stream reader thread
    {
        let b = Arc::clone(&barrier);
        let d = data.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..6 {
                let evs = events::query(&d, events::TODO, usize::MAX).expect("concurrent query");
                assert!(!evs.is_empty());
            }
        }));
    }

    // Compactor thread
    {
        let b = Arc::clone(&barrier);
        let d = data.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            let report = storage_compaction::compact_datastore_lossless(&d, false, false)
                .expect("concurrent compact");
            assert!(report.lossless_verified);
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    let final_journal = todo::task_journal_verify(&data).expect("final verify");
    assert_eq!(final_journal["status"], "ok");
    let conn = Connection::open(events::canonical_db_path(&data)).expect("open db");
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");
    let fk: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .expect("fk check");
    assert_eq!(fk, 0);
}

#[test]
fn compaction_1630_interruption_before_and_after_activation_and_downgrade_guard() {
    let (_tmp, repo, data) = init_store_fixture();
    let t1 = add_sample_task(&data, &repo, "Interrupt Compaction", "desc", "repo");
    todo::edit_task(
        &data,
        &t1,
        Some("Interrupt Compaction v2"),
        None,
        None,
        None,
    )
    .unwrap();

    let db_path = events::canonical_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");

    // Reset v2 activation markers and seed an uncompacted verbose event to test pre-activation crash.
    conn.execute_batch(
        "DROP TABLE IF EXISTS storage_migration_manifests;
         DELETE FROM meta WHERE namespace = 'storage';
         INSERT INTO events(event_id, stream, seq, ts, event_type, actor, payload)
         VALUES('01M85UNCOMPACTED0000000001', 'broker', 999999, '1795400250Z', 'broker.audit', 'agent-a',
                '{\"event_id\":\"01M85UNCOMPACTED0000000001\",\"stream\":\"broker\",\"ts\":\"1795400250Z\",\"event_type\":\"broker.audit\",\"actor\":\"agent-a\",\"actor_id\":\"agent-a\",\"schema_version\":\"1.1.0\",\"db_id\":\"decapod.db\",\"status\":\"success\",\"request_id\":\"\",\"caller_agent_id\":\"\"}');",
    )
    .expect("seed pre-v2 state");

    // 1) Pre-activation simulated crash/ENOSPC: stage v2 compaction inside an explicit
    //    transaction, then roll back before COMMIT (activation).
    conn.execute_batch("BEGIN IMMEDIATE").expect("begin");
    let _manifest_uncommitted =
        storage_compaction::apply_v2_compaction_migration(&conn).expect("stage migration");
    conn.execute_batch("ROLLBACK")
        .expect("simulate crash before activation");

    let has_manifest_table = conn
        .has_table("storage_migration_manifests")
        .expect("has_table");
    assert!(
        !has_manifest_table,
        "pre-activation rollback must leave no partial manifest table"
    );
    let raw_after_rollback: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = '01M85UNCOMPACTED0000000001'",
            [],
            |r| r.get(0),
        )
        .expect("uncompacted row preserved");
    assert!(
        !raw_after_rollback.contains("_ce_v1"),
        "pre-activation rollback must leave legacy payload unrewritten"
    );

    // 2) Re-run after interruption converges cleanly and activates v2 format metadata.
    let applied = storage_compaction::apply_v2_compaction_migration(&conn).expect("converge");
    assert_eq!(applied.post_events_count, applied.pre_events_count);

    // 3) Simulated post-activation interruption (second run / idempotency + downgrade refusal).
    let rerun = storage_compaction::apply_v2_compaction_migration(&conn).expect("idempotent rerun");
    assert_eq!(rerun.events_logical_sha256, applied.events_logical_sha256);

    let downgrade_err =
        storage_compaction::check_storage_format_compatibility_for_version(&conn, "0.138.0", true)
            .expect_err("older binary must refuse v2 compacted datastore");
    assert!(
        downgrade_err
            .to_string()
            .contains("STORAGE_FORMAT_INCOMPATIBLE"),
        "{downgrade_err}"
    );
}

// ============================================================================
// #657 — Knowledge Convergence, Negative Fixtures & Promotion Firewall
// ============================================================================

fn build_knowledge_params<'a>(
    id: &'a str,
    title: &'a str,
    content: &'a str,
    provenance: &'a str,
    merge_key: Option<&'a str>,
    conflict_policy: KnowledgeConflictPolicy,
    status: &'a str,
) -> AddKnowledgeParams<'a> {
    AddKnowledgeParams {
        id,
        title,
        content,
        provenance,
        claim_id: None,
        merge_key,
        conflict_policy,
        status,
        ttl_policy: "persistent",
        expires_ts: None,
    }
}

#[test]
fn knowledge_657_rebuild_vs_incremental_convergence_and_large_stream_pagination() {
    let (_tmp, repo, data) = init_store_fixture();
    let store = Store {
        kind: StoreKind::Repo,
        root: data.clone(),
    };
    let guide = repo.join("DECISION.md");
    fs::write(&guide, "Accept option A over option B.\n").expect("write source");

    // 1) Repeated rejected B (20x), superseded by accepted A.
    let repeated_b = "Prefer rejected design B. ".repeat(20);
    knowledge::add_attested_knowledge(
        &store,
        &repo,
        build_knowledge_params(
            "K_B",
            "Design choice B",
            &repeated_b,
            "file:DECISION.md#L1",
            Some("design-choice"),
            KnowledgeConflictPolicy::Supersede,
            "active",
        ),
        "root",
    )
    .expect("add B");

    knowledge::add_attested_knowledge(
        &store,
        &repo,
        build_knowledge_params(
            "K_A",
            "Design choice A",
            "Accepted design option A.",
            "file:DECISION.md#L1",
            Some("design-choice"),
            KnowledgeConflictPolicy::Supersede,
            "active",
        ),
        "root",
    )
    .expect("supersede B with A");

    // 2) Add and merge-revoke another entry.
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_REV",
            "Design choice revocable",
            "Temporary design advice.",
            "cmd:check",
            Some("revocable-key"),
            KnowledgeConflictPolicy::Merge,
            "active",
        ),
        "root",
    )
    .expect("add revocable");
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_REV_2",
            "Design choice revocable",
            "Revoked design advice.",
            "cmd:check",
            Some("revocable-key"),
            KnowledgeConflictPolicy::Merge,
            "rejected",
        ),
        "root",
    )
    .expect("revoke via merge");

    // 3) Append > 2,100 knowledge retrieval feedback events BEFORE write attestation of K_LATE
    //    to prove keyset pagination in read_attestations_and_feedback_on_conn does not truncate at 2048.
    let db_path = events::canonical_db_path(&data);
    let conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch("BEGIN IMMEDIATE").expect("begin");
    for i in 0..2105usize {
        events::append_on_conn(
            &conn,
            events::KNOWLEDGE,
            &json!({
                "event_id": format!("01M85FB{i:019}"),
                "ts": "1795400300Z",
                "query": "design",
                "returned_ids": ["K_OTHER"],
                "used_ids": ["K_OTHER"],
                "outcome": "neutral",
                "source": "invocation",
            }),
        )
        .expect("append feedback");
    }
    conn.execute_batch("COMMIT").expect("commit");
    drop(conn);

    knowledge::add_attested_knowledge(
        &store,
        &repo,
        build_knowledge_params(
            "K_LATE",
            "Design late attested",
            "Late design entry past 2048 events.",
            "file:DECISION.md#L1",
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .expect("add late attested");

    let incremental = knowledge::resolve_context(&store, &repo, Some("design"), None, &[], None)
        .expect("incremental resolve");

    let ids: Vec<_> = incremental.records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["K_A", "K_LATE"]);
    assert_eq!(incremental.records[0].supersedes_id.as_deref(), Some("K_B"));
    assert_eq!(
        incremental.records[1].freshness, "source_attested_current",
        "attestation past 2048 events must not be truncated"
    );

    // 4) Clean full rebuild into a fresh store from canonical knowledge + events rows must yield
    //    the exact identical KnowledgeContext and source_hash.
    let tmp_rebuilt = tempfile::tempdir().expect("tempdir rebuilt");
    let rebuilt_repo = tmp_rebuilt.path().to_path_buf();
    fs::write(
        rebuilt_repo.join("DECISION.md"),
        "Accept option A over option B.\n",
    )
    .expect("write rebuilt source");
    let rebuilt_data = rebuilt_repo.join(".decapod/data");
    fs::create_dir_all(&rebuilt_data).unwrap();
    todo::initialize_todo_db(&rebuilt_data).unwrap();
    db::initialize_knowledge_db(&rebuilt_data).unwrap();
    federation::initialize_federation_db(&rebuilt_data).unwrap();

    let src_conn = Connection::open(knowledge::knowledge_db_path(&data)).unwrap();
    let dst_conn = Connection::open(knowledge::knowledge_db_path(&rebuilt_data)).unwrap();
    dst_conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    {
        let mut stmt = src_conn
            .prepare("SELECT id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope, status, merge_key, supersedes_id, ttl_policy, expires_ts FROM knowledge ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, String>(11)?,
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, String>(13)?,
                    r.get::<_, Option<String>>(14)?,
                ))
            })
            .unwrap();
        for row in rows {
            let r = row.unwrap();
            dst_conn.execute(
                "INSERT INTO knowledge(id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope, status, merge_key, supersedes_id, ttl_policy, expires_ts)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13, r.14],
            ).unwrap();
        }
        let all_k_events = events::query_all_on_conn(&src_conn, events::KNOWLEDGE).unwrap();
        for ev in all_k_events.into_iter().rev() {
            events::append_on_conn(&dst_conn, events::KNOWLEDGE, &ev.payload).unwrap();
        }
    }
    dst_conn.execute_batch("COMMIT").unwrap();

    let rebuilt_store = Store {
        kind: StoreKind::Repo,
        root: rebuilt_data,
    };
    let rebuilt = knowledge::resolve_context(
        &rebuilt_store,
        &rebuilt_repo,
        Some("design"),
        None,
        &[],
        None,
    )
    .expect("rebuilt resolve");
    assert_eq!(rebuilt, incremental);
    assert_eq!(rebuilt.source_hash, incremental.source_hash);

    // Rejected B remains inspectable via explicit history/get APIs.
    let old_b = knowledge::get_knowledge(&store, "K_B")
        .unwrap()
        .expect("K_B preserved");
    assert_eq!(old_b.status, "superseded");
    assert!(
        !knowledge::knowledge_history(&store, "K_B")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn knowledge_657_negative_fixtures_source_removal_malformed_interrupted_and_scope_bleed() {
    let (_tmp, repo, data) = init_store_fixture();
    let store = Store {
        kind: StoreKind::Repo,
        root: data.clone(),
    };

    // 1) Source removal: attested source file deleted after write.
    let src_file = repo.join("ephemeral_source.md");
    fs::write(&src_file, "design source content").unwrap();
    knowledge::add_attested_knowledge(
        &store,
        &repo,
        build_knowledge_params(
            "K_SRC",
            "Design source",
            "Follow source guidance.",
            "file:ephemeral_source.md",
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .unwrap();
    let before_del =
        knowledge::resolve_context(&store, &repo, Some("design"), None, &[], None).unwrap();
    assert_eq!(before_del.records[0].freshness, "source_attested_current");

    fs::remove_file(&src_file).unwrap();
    let after_del =
        knowledge::resolve_context(&store, &repo, Some("design"), None, &[], None).unwrap();
    assert_eq!(after_del.records[0].freshness, "source_unavailable");

    // 2) Malformed provenance injected into DB must be excluded rather than projected.
    let conn = Connection::open(knowledge::knowledge_db_path(&data)).unwrap();
    conn.execute(
        "INSERT INTO knowledge(id, title, content, provenance, created_at, dir_path, scope, status)
         VALUES('K_MALFORMED', 'Design bad prov', 'Malformed provenance body', 'not-a-valid-scheme', '1795400000Z', '.', 'root', 'active')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO knowledge(id, title, content, provenance, created_at, dir_path, scope, status)
         VALUES('K_TRAVERSAL', 'Design traversal', 'Path traversal prov', 'file:../outside.md', '1795400000Z', '.', 'root', 'active')",
        [],
    )
    .unwrap();
    let with_malformed =
        knowledge::resolve_context(&store, &repo, Some("design"), None, &[], None).unwrap();
    assert_eq!(
        with_malformed.excluded.get("malformed_provenance"),
        Some(&2)
    );
    assert!(
        !with_malformed
            .records
            .iter()
            .any(|r| r.id == "K_MALFORMED" || r.id == "K_TRAVERSAL")
    );

    // 3) Interrupted update (row committed without write_attestation) never claims attestation.
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_UNATTESTED",
            "Design unattested",
            "Committed before attestation",
            "file:ephemeral_source.md",
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .unwrap();
    fs::write(&src_file, "recreated").unwrap();
    let after_recreate =
        knowledge::resolve_context(&store, &repo, Some("design"), None, &[], None).unwrap();
    let unattested_rec = after_recreate
        .records
        .iter()
        .find(|r| r.id == "K_UNATTESTED")
        .expect("unattested record present");
    assert_eq!(
        unattested_rec.provenance_class,
        "recorded_source_unverified"
    );
    assert_eq!(unattested_rec.freshness, "not_captured_at_write");

    // 4) Scope bleed: private, task:other, and path:other cannot leak into task:target.
    for (id, scope) in [
        ("K_PRIV", "private"),
        ("K_OTHER_TASK", "task:other-task"),
        ("K_OTHER_PATH", "path:other/file.rs"),
    ] {
        knowledge::add_scoped_knowledge(
            &store,
            build_knowledge_params(
                id,
                "Design scoped",
                "Scoped secret guidance",
                "cmd:test",
                None,
                KnowledgeConflictPolicy::Reject,
                "active",
            ),
            scope,
        )
        .unwrap();
    }
    let scoped_ctx = knowledge::resolve_context(
        &store,
        &repo,
        Some("design"),
        Some("target-task"),
        &["src/main.rs".to_string()],
        None,
    )
    .unwrap();
    assert!(
        !scoped_ctx
            .records
            .iter()
            .any(|r| matches!(r.id.as_str(), "K_PRIV" | "K_OTHER_TASK" | "K_OTHER_PATH"))
    );
}

#[test]
fn knowledge_657_procedural_promotion_firewall_blocks_obsolete_and_revoked_entries() {
    let (_tmp, _repo, data) = init_store_fixture();
    let store = Store {
        kind: StoreKind::Repo,
        root: data,
    };

    // Seed an active entry, then supersede/revoke it.
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_OLD",
            "Old rule",
            "Deprecated practice B.",
            "cmd:legacy",
            Some("proc-key"),
            KnowledgeConflictPolicy::Supersede,
            "active",
        ),
        "root",
    )
    .unwrap();
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_NEW",
            "New rule",
            "Current practice A.",
            "cmd:current",
            Some("proc-key"),
            KnowledgeConflictPolicy::Supersede,
            "active",
        ),
        "root",
    )
    .unwrap();

    // 1) Promoting superseded K_OLD must fail the firewall.
    let err_superseded = knowledge::record_promotion_event(
        &store,
        KnowledgePromotionEventInput {
            source_entry_id: "K_OLD",
            evidence_refs: &["commit:abc123".to_string()],
            approved_by: "human/reviewer",
            actor: "agent-1",
            reason: "attempt obsolete promotion",
        },
    )
    .expect_err("superseded entry must not promote");
    assert!(
        err_superseded
            .to_string()
            .contains("PROMOTION_FIREWALL_BLOCKED"),
        "{err_superseded}"
    );

    // 2) Create promotion event while K_NEW is active, then revoke K_NEW before procedural add.
    let ev = knowledge::record_promotion_event(
        &store,
        KnowledgePromotionEventInput {
            source_entry_id: "K_NEW",
            evidence_refs: &["commit:def456".to_string()],
            approved_by: "human/reviewer",
            actor: "agent-1",
            reason: "valid while active",
        },
    )
    .expect("active promotion event succeeds");

    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_REVOKER",
            "Revoke rule",
            "Revoked practice.",
            "cmd:revoke",
            Some("proc-key"),
            KnowledgeConflictPolicy::Merge,
            "rejected",
        ),
        "root",
    )
    .unwrap();

    let err_revoked_add = knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "procedural/norms/revoked",
            "Should fail",
            "Revoked procedural rule.",
            &format!("event:{}", ev.event_id),
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .expect_err("procedural add from revoked source must fail firewall");
    assert!(
        err_revoked_add
            .to_string()
            .contains("PROMOTION_FIREWALL_BLOCKED"),
        "{err_revoked_add}"
    );

    // 3) Valid active entry promotes to procedural/, but search_knowledge still treats revoked
    //    entries as absent and procedural entries never bypass KnowledgeContext advisory boundary.
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "K_VALID",
            "Valid rule",
            "Require tests before merge.",
            "cmd:verify",
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .unwrap();
    let valid_ev = knowledge::record_promotion_event(
        &store,
        KnowledgePromotionEventInput {
            source_entry_id: "K_VALID",
            evidence_refs: &["commit:111222".to_string()],
            approved_by: "human/reviewer",
            actor: "agent-1",
            reason: "verified norm",
        },
    )
    .expect("promote active");
    knowledge::add_scoped_knowledge(
        &store,
        build_knowledge_params(
            "procedural/norms/verified",
            "Verified norm",
            "Require tests before merge.",
            &format!("event:{}", valid_ev.event_id),
            None,
            KnowledgeConflictPolicy::Reject,
            "active",
        ),
        "root",
    )
    .unwrap();

    let found = knowledge::search_knowledge(
        &store,
        "Verified norm",
        SearchOptions {
            as_of: None,
            window_days: None,
            rank: "relevance",
        },
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "procedural/norms/verified");
}

#[test]
fn derived_1936_cloud_parity_matrix_and_store_inspection_stay_deterministic_without_sqlite() {
    use decapod::core::cloud_parity_matrix::{
        build_parity_matrix, cross_check_parity_matrix, inspect_cloud_parity,
    };

    let tmp = TempDir::new().unwrap();
    let repo_root = tmp.path().join("cloud-repo");
    fs::create_dir_all(repo_root.join(".decapod")).unwrap();
    fs::write(
        repo_root.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .unwrap();

    let first = inspect_cloud_parity(&repo_root).unwrap();
    let second = inspect_cloud_parity(&repo_root).unwrap();
    assert_eq!(
        first, second,
        "cloud parity derived projection must be deterministic"
    );
    assert_eq!(first.repository_backend, "cloud");
    assert!(!first.local_sqlite_path_present);
    assert!(!first.blanket_parity_claimed);
    assert!(first.cross_check.consistent);
    assert!(first.cross_check.cloud_sqlite_fallback_forbidden);

    let matrix = build_parity_matrix();
    let check = cross_check_parity_matrix(&matrix);
    assert!(check.consistent);
    assert!(
        matrix
            .unsupported_cloud_operations
            .iter()
            .any(|op| op.operation_id == "knowledge.store" && op.never_falls_back_to_sqlite)
    );
    assert!(
        matrix
            .unsupported_cloud_operations
            .iter()
            .any(|op| op.operation_id == "data.compact" && op.never_falls_back_to_sqlite)
    );
    assert!(
        !repo_root.join(".decapod/data/decapod.db").exists(),
        "cloud derived parity inspection must never create decapod.db"
    );
}

#[test]
fn replay_1936_federation_lcm_and_context_capsule_deterministic_rebuild_and_atomic_rollback() {
    use decapod::core::context_capsule;
    use decapod::plugins::lcm;

    let (_tmp, repo, data) = init_store_fixture();
    let store = Store {
        kind: StoreKind::Repo,
        root: data.clone(),
    };
    {
        let reg_conn = Connection::open(federation::federation_db_path(&data)).expect("open db");
        reg_conn
            .execute(
                "INSERT INTO agents(agent_id, trust_level, status, updated_at)
                 VALUES('agent-1936', 'verified', 'active', '1Z')
                 ON CONFLICT(agent_id) DO UPDATE SET trust_level = 'verified'",
                [],
            )
            .expect("register verified test actor");
    }

    // 1) Federation node/edit/edge deterministic rebuild + corrupt-event rollback preservation.
    let n1 = federation::add_node(
        &store,
        "Kernel Decision One",
        "decision",
        "notable",
        "agent_inferred",
        "Use single authoritative SQLite store.",
        "cmd:decapod-test",
        "arch,hardening",
        "repo",
        None,
        "agent-1936",
    )
    .expect("add federation node 1");
    let n2 = federation::add_node(
        &store,
        "Kernel Lesson Two",
        "lesson",
        "background",
        "agent_inferred",
        "Always bind receipts to candidate SHA.",
        "cmd:decapod-test",
        "proof",
        "repo",
        None,
        "agent-1936",
    )
    .expect("add federation node 2");
    federation::edit_node(
        &store,
        &n2.id,
        Some("Kernel Lesson Two Revised"),
        None,
        None,
        None,
    )
    .expect("edit federation node 2");
    federation::add_edge(&store, &n1.id, &n2.id, "depends_on").expect("add federation edge");

    let fed_conn = Connection::open(federation::federation_db_path(&data)).expect("open fed db");
    let snapshot_nodes = |c: &Connection| -> Vec<(String, String, String)> {
        let mut stmt = c
            .prepare("SELECT id, title, status FROM nodes ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let snapshot_edges = |c: &Connection| -> Vec<(String, String, String)> {
        let mut stmt = c
            .prepare("SELECT source_id, target_id, edge_type FROM node_edges ORDER BY source_id, target_id, edge_type")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let expected_nodes = snapshot_nodes(&fed_conn);
    let expected_edges = snapshot_edges(&fed_conn);

    // Wipe derived federation tables and rebuild from canonical events stream.
    fed_conn
        .execute_batch("DELETE FROM node_edges; DELETE FROM nodes;")
        .expect("wipe derived federation tables");
    let replayed_count = federation::rebuild_from_events(&data).expect("rebuild federation");
    assert_eq!(replayed_count, 4);
    assert_eq!(snapshot_nodes(&fed_conn), expected_nodes);
    assert_eq!(snapshot_edges(&fed_conn), expected_edges);

    // Inject a corrupt federation event (missing required node_type) -> rebuild must roll back
    // and leave the pre-rebuild derived projection completely intact.
    fed_conn
        .execute(
            "INSERT INTO events(seq, event_id, ts, stream, event_type, actor, subject_kind, subject_id, payload)
             VALUES((SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE stream='federation'),
                    '01M85CORRUPTFED00000000001', '1795400400Z', 'federation', 'node.create',
                    'agent-1936', 'node', 'N_CORRUPT', '{\"title\":\"broken\"}')",
            [],
        )
        .expect("insert corrupt federation event");
    let fed_err = federation::rebuild_from_events(&data).expect_err("corrupt event must roll back");
    assert!(
        fed_err
            .to_string()
            .contains("pre-rebuild projection preserved"),
        "{fed_err}"
    );
    assert_eq!(snapshot_nodes(&fed_conn), expected_nodes);
    assert_eq!(snapshot_edges(&fed_conn), expected_edges);
    drop(fed_conn);

    // 2) LCM deterministic rebuild & atomic rollback when ledger integrity fails (#1936 fix).
    lcm::initialize_lcm_db(&data).expect("init lcm db");
    let r1 = lcm::ingest(
        &store,
        "first immutable artifact",
        "artifact",
        "agent-1936",
        Some("sess-1"),
        Some("test"),
    )
    .expect("lcm ingest 1");
    let r2 = lcm::ingest(
        &store,
        "second immutable event",
        "event",
        "agent-1936",
        Some("sess-1"),
        Some("test"),
    )
    .expect("lcm ingest 2");
    assert_eq!(
        lcm::rebuild_index(&store, true).expect("clean lcm rebuild")["status"],
        "success"
    );
    let originals_before = lcm::list_originals(&store, None, Some(10)).expect("list originals");
    assert_eq!(originals_before.len(), 2);

    // Tamper with one LCM ledger event's content_hash -> validated rebuild MUST fail AND
    // preserve the existing `originals_index` intact rather than wiping it!
    let lcm_conn = Connection::open(events::canonical_db_path(&data)).expect("open lcm db");
    let ev1_id = r1["event_id"].as_str().expect("ev1 id");
    let ev2_id = r2["event_id"].as_str().expect("ev2 id");
    let _ = ev1_id;
    let raw_payload: String = lcm_conn
        .query_row(
            "SELECT payload FROM events WHERE stream='lcm' AND event_id=?1",
            [ev2_id],
            |r| r.get(0),
        )
        .expect("read lcm payload");
    let mut parsed_ev: serde_json::Value = serde_json::from_str(&raw_payload).unwrap();
    parsed_ev["content_hash"] = json!("0".repeat(64));
    lcm_conn
        .execute(
            "UPDATE events SET payload = ?1 WHERE stream='lcm' AND event_id=?2",
            params![parsed_ev.to_string(), ev2_id],
        )
        .expect("tamper lcm event hash");
    drop(lcm_conn);

    let failed_rebuild = lcm::rebuild_index(&store, true).expect("failed validated rebuild");
    assert_eq!(failed_rebuild["status"], "failed");
    assert_eq!(failed_rebuild["error_count"], 1);
    let originals_after_fail =
        lcm::list_originals(&store, None, Some(10)).expect("originals preserved");
    assert_eq!(
        originals_after_fail.len(),
        originals_before.len(),
        "validated lcm::rebuild_index failure must preserve prior originals_index"
    );

    // 3) DeterministicContextCapsule query parity across repeated evaluations.
    let cap_a =
        context_capsule::query_embedded_capsule(&repo, "validation", "interfaces", None, None, 5)
            .expect("capsule a");
    let cap_b =
        context_capsule::query_embedded_capsule(&repo, "validation", "interfaces", None, None, 5)
            .expect("capsule b");
    assert_eq!(cap_a.capsule_hash, cap_b.capsule_hash);
    assert_eq!(
        cap_a.canonical_json_bytes().unwrap(),
        cap_b.canonical_json_bytes().unwrap()
    );
}
