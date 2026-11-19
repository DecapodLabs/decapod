//! Integration & reproducible benchmark proof suite for Decapod #1734:
//! Projection performance budgets under large repositories and concurrent agents.
//!
//! Proves:
//! 1. Covering SQLite indexes eliminate `USE TEMP B-TREE FOR ORDER BY` on
//!    stream and subject-scoped cursor replay, and `events::actors` avoids
//!    loading full event payloads.
//! 2. Bounded response size, row, scan, worktree, file, and wall-time budgets
//!    enter explicit typed degraded mode rather than truncating silently or
//!    starving governed writers.
//! 3. Concurrent governed WAL writers and read-only projection clients operate
//!    without writer disruption, lock starvation, or sequence gaps/duplicates.
//! 4. Logical projection receipts remain bit-for-bit identical across lossless
//!    v2 compaction, component GC, explicit VACUUM, and bounded event retention,
//!    while stale cursors behind a retention floor degrade honestly with a
//!    verifiable cryptographic retention anchor.
//! 5. Strictly read-only kernel boundary: zero UI dependencies, `read_only=true`,
//!    `authoritative=false`, and zero datastore mutations during projection reads.

use decapod::core::db::Connection;
use decapod::core::events;
use decapod::core::projection::{
    CursorDisposition, DegradedReason, EventProjectionQuery, PROJECTION_BUDGET_SCHEMA_VERSION,
    ProjectionBudget, ProjectionStatus, WorktreeProjectionQuery, apply_bounded_event_retention,
    project_events_on_conn, project_events_on_conn_with_deadline, project_events_read_only,
    project_worktrees_read_only, project_worktrees_read_only_with_deadline, read_retention_anchor,
};
use decapod::core::schemas;
use decapod::core::storage_compaction;
use decapod::core::storage_lock::{StorageLock, StorageLockMode};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

fn explain_query_plan(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    stmt.query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn percentile_us(samples_ns: &mut [u128], pct: usize) -> u64 {
    if samples_ns.is_empty() {
        return 0;
    }
    samples_ns.sort_unstable();
    let idx = (samples_ns.len() * pct).div_ceil(100).saturating_sub(1);
    (samples_ns[idx] / 1_000) as u64
}

fn git_cmd(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Perf Fixture")
        .env("GIT_COMMITTER_NAME", "Perf Fixture")
        .env("GIT_AUTHOR_EMAIL", "perf@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "perf@example.invalid")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn setup_git_repo_with_worktrees(worktree_count: usize) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git_cmd(&repo, &["init", "-b", "master"]);
    fs::write(repo.join("README.md"), "# Perf Fixture\n").unwrap();
    git_cmd(&repo, &["add", "README.md"]);
    git_cmd(&repo, &["commit", "-m", "init"]);

    let wt_root = dir.path().join("worktrees");
    fs::create_dir_all(&wt_root).unwrap();
    for i in 0..worktree_count {
        let wt_path = wt_root.join(format!("wt-{i:03}"));
        let branch = format!("agent/bench/task-{i:03}");
        git_cmd(
            &repo,
            &["worktree", "add", "-b", &branch, wt_path.to_str().unwrap()],
        );
    }
    (dir, repo)
}

#[test]
fn sqlite_covering_indexes_eliminate_temp_btree_on_cursor_and_subject_queries() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db_path).unwrap();
    events::ensure_tables(&conn).unwrap();

    for i in 0..200u64 {
        let task_id = format!("task_{:03}", i % 10);
        let actor = format!("agent-{}", i % 5);
        events::append_on_conn(
            &conn,
            events::TODO,
            &json!({
                "event_id": format!("evt-{i:06}"),
                "ts": "2026-11-18T00:00:00Z",
                "event_type": "task.update",
                "task_id": task_id,
                "actor": actor,
                "payload": {"step": i, "note": "x".repeat(256)}
            }),
        )
        .unwrap();
    }

    let stream_plan = explain_query_plan(
        &conn,
        "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
         FROM events WHERE stream = 'todo' AND seq > 10 ORDER BY seq ASC LIMIT 64",
    );
    let stream_plan_text = stream_plan.join(" | ");
    assert!(
        stream_plan_text.contains("idx_events_stream_seq_unique"),
        "expected stream cursor query to use idx_events_stream_seq_unique, got: {stream_plan_text}"
    );
    assert!(
        !stream_plan_text.contains("TEMP B-TREE"),
        "stream cursor query must not sort in a temp B-tree: {stream_plan_text}"
    );

    let subject_plan = explain_query_plan(
        &conn,
        "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
         FROM events
         WHERE stream = 'todo' AND subject_kind = 'task' AND subject_id = 'task_003'
           AND subject_kind IS NOT NULL AND subject_id IS NOT NULL
           AND seq > 0
         ORDER BY seq ASC LIMIT 64",
    );
    let subject_plan_text = subject_plan.join(" | ");
    assert!(
        subject_plan_text.contains("idx_events_stream_subject_seq"),
        "expected subject cursor query to use idx_events_stream_subject_seq, got: {subject_plan_text}"
    );
    assert!(
        !subject_plan_text.contains("TEMP B-TREE"),
        "subject cursor query must not sort in a temp B-tree: {subject_plan_text}"
    );

    // Distinct actor query must return exact sorted distinct actors without loading payloads.
    let actors = events::actors(&data_root, events::TODO).unwrap();
    assert_eq!(
        actors,
        vec!["agent-0", "agent-1", "agent-2", "agent-3", "agent-4"]
    );
}

#[test]
fn bounded_event_cursor_replay_and_degraded_modes() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db_path).unwrap();
    events::ensure_tables(&conn).unwrap();

    for i in 1..=120u64 {
        let etype = if i % 40 == 0 {
            "task.rare_checkpoint"
        } else {
            "task.heartbeat"
        };
        events::append_on_conn(
            &conn,
            events::TODO,
            &json!({
                "event_id": format!("evt-{i:04}"),
                "ts": "2026-11-18T00:00:00Z",
                "event_type": etype,
                "task_id": format!("task_{:02}", i % 3),
                "actor": "agent-alpha",
                "payload": {"seq_hint": i, "padding": "p".repeat(512)}
            }),
        )
        .unwrap();
    }

    // 1. Paginated cursor replay reconstructs all 120 events cleanly with zero gaps/duplicates
    //    and never mutates the underlying SQLite file.
    let db_bytes_before = fs::read(&db_path).unwrap();
    let mut after_seq = 0u64;
    let mut seen_seqs = Vec::new();
    let mut pages = 0usize;
    loop {
        let mut query = EventProjectionQuery::for_stream(events::TODO);
        query.after_seq = after_seq;
        query.budget.max_rows = 25;
        query.budget.max_response_bytes = 256 * 1024;
        let page = project_events_read_only(&data_root, &query).unwrap();
        assert_eq!(page.schema_version, PROJECTION_BUDGET_SCHEMA_VERSION);
        assert!(page.read_only);
        assert!(!page.authoritative);
        assert!(page.metrics.response_bytes <= query.budget.max_response_bytes);
        assert!(page.items.len() <= query.budget.max_rows);
        for item in &page.items {
            seen_seqs.push(item.seq);
        }
        pages += 1;
        if !page.cursor.has_more {
            assert_eq!(page.status, ProjectionStatus::Complete);
            break;
        }
        assert_eq!(page.status, ProjectionStatus::PartialHasMore);
        assert!(page.cursor.next_seq > after_seq);
        after_seq = page.cursor.next_seq;
    }
    assert_eq!(pages, 5);
    assert_eq!(seen_seqs, (1..=120).collect::<Vec<_>>());
    assert_eq!(
        fs::read(&db_path).unwrap(),
        db_bytes_before,
        "read-only event projection must never mutate canonical SQLite bytes"
    );

    // 2. Tight response-byte budget enters explicit Degraded mode before exceeding cap.
    let mut tight_bytes = EventProjectionQuery::for_stream(events::TODO);
    tight_bytes.budget.max_rows = 100;
    tight_bytes.budget.max_response_bytes = 2_048;
    let degraded = project_events_read_only(&data_root, &tight_bytes).unwrap();
    assert_eq!(degraded.status, ProjectionStatus::Degraded);
    assert!(degraded.cursor.has_more);
    assert!(degraded.metrics.response_bytes <= 2_048);
    assert!(
        degraded
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::ResponseByteBudgetExceeded { .. }))
    );
    // Resuming from the degraded cursor makes strict forward progress with no overlap.
    tight_bytes.after_seq = degraded.cursor.next_seq;
    let resumed = project_events_read_only(&data_root, &tight_bytes).unwrap();
    assert!(resumed.items.first().unwrap().seq > degraded.items.last().unwrap().seq);

    // 3. Sparse event_type filter trips ScanBudgetExceeded instead of unbounded table scan.
    let mut sparse = EventProjectionQuery::for_stream(events::TODO);
    sparse.event_type = Some("task.rare_checkpoint".to_string());
    sparse.budget.max_scan_rows = 20;
    sparse.budget.max_rows = 10;
    let scan_capped = project_events_read_only(&data_root, &sparse).unwrap();
    assert_eq!(scan_capped.status, ProjectionStatus::Degraded);
    assert!(scan_capped.cursor.has_more);
    assert_eq!(scan_capped.metrics.rows_scanned, 20);
    assert!(scan_capped.cursor.next_seq >= 20);
    assert!(
        scan_capped
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::ScanBudgetExceeded { .. }))
    );

    // 4. Wall-time budget trips deterministically when deadline is exhausted.
    let now = Instant::now();
    let timed = project_events_on_conn_with_deadline(
        &conn,
        &EventProjectionQuery::for_stream(events::TODO),
        now,
        now,
    )
    .unwrap();
    assert_eq!(timed.status, ProjectionStatus::Degraded);
    assert!(
        timed
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::WallTimeBudgetExceeded { .. }))
    );

    // 5. Exclusive maintenance lock triggers graceful ReaderContentionDeferred without stalling.
    let excl = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::from_secs(2))
        .expect("acquire exclusive maintenance lock");
    let contended_page = thread::spawn({
        let data_root = data_root.clone();
        move || {
            let mut q = EventProjectionQuery::for_stream(events::TODO);
            q.budget.reader_lock_timeout_ms = 60;
            let t0 = Instant::now();
            let page = project_events_read_only(&data_root, &q).unwrap();
            (page, t0.elapsed())
        }
    })
    .join()
    .unwrap();
    drop(excl);
    assert_eq!(contended_page.0.status, ProjectionStatus::Degraded);
    assert!(
        contended_page
            .0
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::ReaderContentionDeferred { .. })),
        "expected ReaderContentionDeferred, got {:?}",
        contended_page.0.degraded_reasons
    );
    assert!(
        contended_page.1 < Duration::from_secs(2),
        "degraded contention check must return promptly, took {:?}",
        contended_page.1
    );
}

#[test]
fn concurrent_wal_writers_and_readonly_projection_readers_no_disruption() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .unwrap();
        events::ensure_tables(&conn).unwrap();
    }

    let writer_threads = 4usize;
    let events_per_writer = 60usize;
    let expected_total = writer_threads * events_per_writer;

    // Run 4 concurrent governed writers alongside 4 continuous read-only projection clients.
    let writers_done = Arc::new(AtomicBool::new(false));
    let reader_pages_completed = Arc::new(AtomicUsize::new(0));
    let reader_degraded_contention = Arc::new(AtomicUsize::new(0));
    let start_barrier = Arc::new(Barrier::new(writer_threads + 4));

    let mut writer_latencies_ns = Vec::new();
    thread::scope(|scope| {
        let mut w_handles = Vec::new();
        for w in 0..writer_threads {
            let db_path = db_path.clone();
            let barrier = Arc::clone(&start_barrier);
            w_handles.push(scope.spawn(move || {
                let conn = Connection::open_with_options(
                    &db_path,
                    dactyl_db::AccessMode::ReadWrite,
                    Duration::from_secs(5),
                )
                .unwrap();
                let mut lats = Vec::with_capacity(events_per_writer);
                barrier.wait();
                for i in 0..events_per_writer {
                    let ev = json!({
                        "event_id": format!("conc-w{w}-e{i:04}"),
                        "ts": "2026-11-18T00:00:00Z",
                        "event_type": "task.claim",
                        "task_id": format!("task_w{w}"),
                        "actor": format!("writer-{w}"),
                        "payload": {"writer": w, "iter": i, "detail": "d".repeat(256)}
                    });
                    let t0 = Instant::now();
                    let seq = events::append_on_conn(&conn, events::TODO, &ev)
                        .expect("governed writer must never fail under read-only observer load");
                    lats.push(t0.elapsed().as_nanos());
                    assert!(seq > 0);
                }
                lats
            }));
        }

        let mut r_handles = Vec::new();
        for _r in 0..4usize {
            let data_root = data_root.clone();
            let done = Arc::clone(&writers_done);
            let pages_ok = Arc::clone(&reader_pages_completed);
            let contended = Arc::clone(&reader_degraded_contention);
            let barrier = Arc::clone(&start_barrier);
            r_handles.push(scope.spawn(move || {
                let mut after_seq = 0u64;
                let mut seen = BTreeSet::new();
                barrier.wait();
                loop {
                    let mut q = EventProjectionQuery::for_stream(events::TODO);
                    q.after_seq = after_seq;
                    q.budget.max_rows = 32;
                    q.budget.max_response_bytes = 64 * 1024;
                    q.budget.reader_lock_timeout_ms = 200;
                    let page = project_events_read_only(&data_root, &q).unwrap();
                    assert!(page.read_only);
                    assert!(!page.authoritative);
                    assert!(page.metrics.response_bytes <= q.budget.max_response_bytes);

                    if page
                        .degraded_reasons
                        .iter()
                        .any(|r| matches!(r, DegradedReason::ReaderContentionDeferred { .. }))
                    {
                        contended.fetch_add(1, Ordering::Relaxed);
                    } else {
                        for item in &page.items {
                            assert!(
                                item.seq > after_seq,
                                "non-monotonic cursor seq {} <= {}",
                                item.seq,
                                after_seq
                            );
                            assert!(
                                seen.insert(item.seq),
                                "duplicate seq {} across incremental cursor replay",
                                item.seq
                            );
                        }
                        after_seq = page.cursor.next_seq;
                        pages_ok.fetch_add(1, Ordering::Relaxed);
                    }

                    if done.load(Ordering::Acquire) && !page.cursor.has_more {
                        // One final catch-up read after writers finished.
                        let mut final_q = EventProjectionQuery::for_stream(events::TODO);
                        final_q.after_seq = after_seq;
                        final_q.budget.max_rows = 512;
                        let final_page = project_events_read_only(&data_root, &final_q).unwrap();
                        for item in &final_page.items {
                            seen.insert(item.seq);
                        }
                        if !final_page.cursor.has_more {
                            break;
                        }
                        after_seq = final_page.cursor.next_seq;
                    }
                }
                seen
            }));
        }

        for h in w_handles {
            writer_latencies_ns.extend(h.join().unwrap());
        }
        writers_done.store(true, Ordering::Release);
        for h in r_handles {
            let seen = h.join().unwrap();
            assert_eq!(
                seen.len(),
                expected_total,
                "incremental read client must observe every committed event exactly once"
            );
        }
    });

    assert_eq!(writer_latencies_ns.len(), expected_total);
    assert!(reader_pages_completed.load(Ordering::Relaxed) >= 4);
    let _ = reader_degraded_contention.load(Ordering::Relaxed);

    // Verify canonical DB still holds exact expected count and passes integrity check.
    let verify_conn = Connection::open(&db_path).unwrap();
    let count: u64 = verify_conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'todo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, expected_total as u64);
    let integrity: String = verify_conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn receipts_remain_stable_across_compaction_gc_vacuum_and_retention() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db_path).unwrap();
    events::ensure_tables(&conn).unwrap();
    conn.execute_batch(schemas::TODO_DB_SCHEMA_META).unwrap();
    conn.execute_batch(schemas::STORAGE_COMPONENTS_SCHEMA)
        .unwrap();
    conn.execute_batch(schemas::STORAGE_COMPONENT_REFS_SCHEMA)
        .unwrap();
    conn.execute_batch(schemas::STORAGE_MIGRATION_MANIFESTS_SCHEMA)
        .unwrap();

    // Seed 100 legacy non-compact JSON event rows (duplicating envelope columns inside payload).
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    for i in 1..=100u64 {
        let event_id = format!("legacy-evt-{i:04}");
        let ts = "2026-11-18T00:00:00Z";
        let task_id = format!("task_{:02}", i % 5);
        let raw_legacy_payload = json!({
            "event_id": event_id,
            "ts": ts,
            "event_type": "task.journal.checkpoint",
            "actor": "agent-compact",
            "task_id": task_id,
            "payload": {"step": i, "summary": "Governed compaction continuity payload"}
        })
        .to_string();
        conn.execute(
            "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
             VALUES(?1, ?2, ?3, 'todo', 'task', ?4, 'task.journal.checkpoint', ?5, 'agent-compact')",
            decapod::params![event_id, ts, i, task_id, raw_legacy_payload],
        )
        .unwrap();
    }
    // Insert one unreferenced storage component for GC verification.
    conn.execute(
        "INSERT INTO storage_components(component_kind, codec_version, byte_len, content_sha256, stored_bytes, created_at)
         VALUES('journal_note', 1, 12, 'deadbeef', X'68656c6c6f20776f726c6421', '2026-11-18T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute_batch("COMMIT").unwrap();

    // Capture baseline full-stream and tail-window (seq 61..=100) projection receipts.
    let full_query = EventProjectionQuery::for_stream(events::TODO);
    let mut tail_query = EventProjectionQuery::for_stream(events::TODO);
    tail_query.after_seq = 60;

    let baseline_full = project_events_on_conn(&conn, &full_query).unwrap();
    let baseline_tail = project_events_on_conn(&conn, &tail_query).unwrap();
    assert_eq!(baseline_full.status, ProjectionStatus::Complete);
    assert_eq!(baseline_full.items.len(), 100);
    assert_eq!(baseline_tail.items.len(), 40);

    // 1. Run v2 compaction migration + component GC + explicit VACUUM.
    let compaction = storage_compaction::apply_v2_compaction_migration(&conn).unwrap();
    assert!(
        compaction.event_compaction.compacted_rows > 0,
        "expected legacy events to compact into _ce_v1 envelopes"
    );
    let gc = storage_compaction::gc_unreferenced_components(&conn, false).unwrap();
    assert_eq!(gc.deleted_components, 1);
    let vacuum = storage_compaction::reclaim_pages_explicit(&conn).unwrap();
    assert!(vacuum.pages_after <= vacuum.pages_before);

    // Logical receipts must be bit-for-bit identical after physical compaction, GC, and VACUUM.
    let after_compaction_full = project_events_on_conn(&conn, &full_query).unwrap();
    let after_compaction_tail = project_events_on_conn(&conn, &tail_query).unwrap();
    assert_eq!(
        after_compaction_full.content_sha256,
        baseline_full.content_sha256
    );
    assert_eq!(
        after_compaction_full.window_receipt_sha256,
        baseline_full.window_receipt_sha256
    );
    assert_eq!(
        after_compaction_full.continuity_receipt_sha256,
        baseline_full.continuity_receipt_sha256
    );
    assert_eq!(after_compaction_full.items, baseline_full.items);
    assert_eq!(
        after_compaction_tail.window_receipt_sha256,
        baseline_tail.window_receipt_sha256
    );

    // 2. Apply explicit bounded event retention: keep latest 40 events (prune seq 1..=60).
    let retention = apply_bounded_event_retention(&conn, events::TODO, 40).unwrap();
    assert_eq!(retention.pruned_rows, 60);
    assert_eq!(retention.retained_rows, 40);
    assert_eq!(retention.retention_floor_seq, 60);
    let anchor1 = retention.anchor.expect("retention anchor created");
    assert_eq!(anchor1.cumulative_pruned_count, 60);
    assert!(anchor1.prev_anchor_sha256.is_none());

    // Retained window cursor (after_seq = 60) preserves EXACT content_sha256 & window_receipt_sha256!
    let post_retention_tail = project_events_on_conn(&conn, &tail_query).unwrap();
    assert_eq!(post_retention_tail.status, ProjectionStatus::Complete);
    assert_eq!(
        post_retention_tail.cursor.disposition,
        CursorDisposition::Continuous
    );
    assert_eq!(post_retention_tail.cursor.retention_floor_seq, 60);
    assert_eq!(
        post_retention_tail
            .cursor
            .retention_anchor_sha256
            .as_deref(),
        Some(anchor1.anchor_sha256.as_str())
    );
    assert_eq!(
        post_retention_tail.content_sha256,
        baseline_tail.content_sha256
    );
    assert_eq!(
        post_retention_tail.window_receipt_sha256,
        baseline_tail.window_receipt_sha256
    );

    // Fresh cursor (after_seq = 0) reports StartsAtRetentionFloor honestly.
    let fresh_after_retention = project_events_on_conn(&conn, &full_query).unwrap();
    assert_eq!(fresh_after_retention.status, ProjectionStatus::Complete);
    assert_eq!(
        fresh_after_retention.cursor.disposition,
        CursorDisposition::StartsAtRetentionFloor
    );
    assert_eq!(fresh_after_retention.items.len(), 40);
    assert_eq!(
        fresh_after_retention.window_receipt_sha256,
        baseline_tail.window_receipt_sha256
    );

    // Stale cursor (0 < after_seq < 60) enters explicit Degraded mode with CursorBehindRetentionFloor.
    let mut stale_query = EventProjectionQuery::for_stream(events::TODO);
    stale_query.after_seq = 25;
    let stale_page = project_events_on_conn(&conn, &stale_query).unwrap();
    assert_eq!(stale_page.status, ProjectionStatus::Degraded);
    assert_eq!(
        stale_page.cursor.disposition,
        CursorDisposition::BehindRetentionFloor
    );
    assert!(stale_page.degraded_reasons.iter().any(|r| matches!(
        r,
        DegradedReason::CursorBehindRetentionFloor {
            requested_seq: 25,
            retention_floor_seq: 60
        }
    )));
    assert_eq!(
        stale_page.window_receipt_sha256,
        baseline_tail.window_receipt_sha256
    );

    // 3. Second retention pass (full prune keep_latest = 0) chains anchor & keeps future seq monotonic!
    let full_prune = apply_bounded_event_retention(&conn, events::TODO, 0).unwrap();
    assert_eq!(full_prune.pruned_rows, 40);
    assert_eq!(full_prune.retained_rows, 0);
    assert_eq!(full_prune.retention_floor_seq, 100);
    let anchor2 = read_retention_anchor(&conn, events::TODO)
        .unwrap()
        .expect("chained retention anchor");
    assert_eq!(anchor2.cumulative_pruned_count, 100);
    assert_eq!(
        anchor2.prev_anchor_sha256.as_deref(),
        Some(anchor1.anchor_sha256.as_str())
    );

    // Appending after full prune must allocate seq = 101 (never colliding with 1..=100).
    let next_seq = events::append_on_conn(
        &conn,
        events::TODO,
        &json!({
            "event_id": "post-retention-evt-101",
            "ts": "2026-11-18T00:01:00Z",
            "event_type": "task.add",
            "task_id": "task_new",
            "actor": "agent-compact"
        }),
    )
    .unwrap();
    assert_eq!(next_seq, 101);
}

#[test]
fn bounded_worktree_enumeration_and_degraded_mode() {
    let (_tmp, repo) = setup_git_repo_with_worktrees(10);
    let total_expected = 11usize; // main repo + 10 linked worktrees

    // Create 15 untracked files in wt-000 to verify bounded file enumeration cap.
    let wt0 = repo.parent().unwrap().join("worktrees/wt-000");
    for i in 0..15 {
        fs::write(wt0.join(format!("untracked-{i:02}.txt")), "dirty\n").unwrap();
    }

    // 1. Paginate across all worktrees with max_worktrees = 4.
    let mut cursor: Option<String> = None;
    let mut collected = Vec::new();
    let mut saw_worktree_budget_degraded = false;
    loop {
        let mut q = WorktreeProjectionQuery {
            after_worktree_path: cursor.clone(),
            include_dirty_summary: false,
            budget: ProjectionBudget::default(),
        };
        q.budget.max_worktrees = 4;
        let page = project_worktrees_read_only(&repo, &q).unwrap();
        assert!(page.read_only);
        assert!(!page.authoritative);
        assert_eq!(page.total_discovered, total_expected);
        assert!(page.items.len() <= 4);
        if page
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::WorktreeBudgetExceeded { .. }))
        {
            saw_worktree_budget_degraded = true;
        }
        collected.extend(page.items.into_iter().map(|e| e.path));
        if !page.has_more {
            assert_eq!(page.status, ProjectionStatus::Complete);
            break;
        }
        cursor = page.next_cursor;
        assert!(cursor.is_some());
    }
    assert!(saw_worktree_budget_degraded);
    assert_eq!(collected.len(), total_expected);
    let unique: BTreeSet<&String> = collected.iter().collect();
    assert_eq!(unique.len(), total_expected);

    // 2. Dirty summary enforces max_files_per_worktree and flags FileEnumerationBudgetExceeded.
    let mut dirty_q = WorktreeProjectionQuery {
        after_worktree_path: None,
        include_dirty_summary: true,
        budget: ProjectionBudget::default(),
    };
    dirty_q.budget.max_worktrees = 32;
    dirty_q.budget.max_files_per_worktree = 5;
    let dirty_page = project_worktrees_read_only(&repo, &dirty_q).unwrap();
    assert_eq!(dirty_page.status, ProjectionStatus::Degraded);
    assert!(dirty_page.degraded_reasons.iter().any(|r| matches!(
        r,
        DegradedReason::FileEnumerationBudgetExceeded {
            budget_files: 5,
            observed_files: 15,
            ..
        }
    )));
    let wt0_entry = dirty_page
        .items
        .iter()
        .find(|e| e.path.ends_with("wt-000"))
        .expect("wt-000 entry present");
    assert_eq!(wt0_entry.dirty_file_count, Some(15));
    assert!(wt0_entry.dirty_truncated);
    assert_eq!(wt0_entry.sample_dirty_paths.len(), 5);

    // 3. Deterministic wall-time deadline exhaustion on worktree enumeration.
    let now = Instant::now();
    let timed = project_worktrees_read_only_with_deadline(
        &repo,
        &WorktreeProjectionQuery::default(),
        now,
        now,
    )
    .unwrap();
    assert_eq!(timed.status, ProjectionStatus::Degraded);
    assert!(
        timed
            .degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::WallTimeBudgetExceeded { .. }))
    );
}

/// Reproducible measured-baseline benchmark for #1734.
/// Run explicitly with:
/// `cargo test --test projection_performance_budgets_1734 -- --ignored --nocapture`
#[test]
#[ignore = "explicit reproducible projection benchmark"]
fn projection_performance_baseline_benchmark_1734() {
    // --- Benchmark 1: 10,000 events — unindexed subject sort vs covering index + bounded cursor ---
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
        .unwrap();
    events::ensure_tables(&conn).unwrap();

    let total_events = 10_000u64;
    let payload_body = "x".repeat(512);
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    for i in 1..=total_events {
        let task_id = format!("task_{:03}", i % 50);
        let actor = format!("agent-{:02}", i % 16);
        let payload = json!({"_ce_v1": 1, "step": i, "body": payload_body}).to_string();
        conn.execute(
            "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
             VALUES(?1, '2026-11-18T00:00:00Z', ?2, 'todo', 'task', ?3, 'task.checkpoint', ?4, ?5)",
            decapod::params![format!("bench-{i:06}"), i, task_id, payload, actor],
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();

    // Measure BEFORE dropping/restoring idx_events_stream_subject_seq:
    let subject_sql =
        "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
                       FROM events
                       WHERE stream = 'todo' AND subject_kind = 'task' AND subject_id = 'task_007'
                         AND subject_kind IS NOT NULL AND subject_id IS NOT NULL
                         AND seq > 0
                       ORDER BY seq ASC LIMIT 64";
    let indexed_plan = explain_query_plan(&conn, subject_sql);
    let mut indexed_subject_ns = Vec::new();
    for _ in 0..50 {
        let mut q = EventProjectionQuery::for_stream(events::TODO);
        q.subject_kind = Some("task".to_string());
        q.subject_id = Some("task_007".to_string());
        q.budget.max_rows = 64;
        let t0 = Instant::now();
        let p = project_events_on_conn(&conn, &q).unwrap();
        indexed_subject_ns.push(t0.elapsed().as_nanos());
        assert_eq!(p.items.len(), 64);
    }

    conn.execute_batch("DROP INDEX IF EXISTS idx_events_stream_subject_seq;")
        .unwrap();
    let unindexed_plan = explain_query_plan(&conn, subject_sql);
    let mut unindexed_subject_ns = Vec::new();
    for _ in 0..50 {
        let mut q = EventProjectionQuery::for_stream(events::TODO);
        q.subject_kind = Some("task".to_string());
        q.subject_id = Some("task_007".to_string());
        q.budget.max_rows = 64;
        let t0 = Instant::now();
        let p = project_events_on_conn(&conn, &q).unwrap();
        unindexed_subject_ns.push(t0.elapsed().as_nanos());
        assert_eq!(p.items.len(), 64);
    }
    conn.execute(schemas::EVENTS_TABLE_STREAM_SUBJECT_SEQ_INDEX, [])
        .unwrap();

    // Measure unbounded query(usize::MAX) vs default bounded page (256 rows / 512 KiB).
    let t_unbounded = Instant::now();
    let all_rows = events::query(&data_root, events::TODO, usize::MAX).unwrap();
    let unbounded_ms = t_unbounded.elapsed().as_millis() as u64;
    let unbounded_bytes: usize = all_rows
        .iter()
        .map(|e| serde_json::to_vec(e).unwrap().len())
        .sum();

    let mut bounded_page_ns = Vec::new();
    let mut last_bounded_bytes = 0usize;
    let mut last_peak_batch_bytes = 0usize;
    for _ in 0..30 {
        let q = EventProjectionQuery::for_stream(events::TODO);
        let t0 = Instant::now();
        let p = project_events_read_only(&data_root, &q).unwrap();
        bounded_page_ns.push(t0.elapsed().as_nanos());
        last_bounded_bytes = p.metrics.response_bytes;
        last_peak_batch_bytes = p.metrics.peak_batch_bytes;
    }

    let mut actors_ns = Vec::new();
    for _ in 0..30 {
        let t0 = Instant::now();
        let a = events::actors(&data_root, events::TODO).unwrap();
        actors_ns.push(t0.elapsed().as_nanos());
        assert_eq!(a.len(), 16);
    }

    // --- Benchmark 2: Concurrent WAL writers (solo vs with 4 active read projection clients) ---
    let measure_writers = |with_readers: bool| -> (Vec<u128>, usize, usize) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(schemas::LOCAL_DB_NAME);
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
                .unwrap();
            events::ensure_tables(&c).unwrap();
        }
        let done = Arc::new(AtomicBool::new(false));
        let reader_pages = Arc::new(AtomicUsize::new(0));
        let reader_contended = Arc::new(AtomicUsize::new(0));
        let mut writer_ns = Vec::new();
        thread::scope(|scope| {
            let mut r_handles = Vec::new();
            if with_readers {
                for _ in 0..4 {
                    let root = root.clone();
                    let done = Arc::clone(&done);
                    let pages = Arc::clone(&reader_pages);
                    let cont = Arc::clone(&reader_contended);
                    r_handles.push(scope.spawn(move || {
                        let mut cursor = 0u64;
                        while !done.load(Ordering::Acquire) {
                            let mut q = EventProjectionQuery::for_stream(events::TODO);
                            q.after_seq = cursor;
                            let p = project_events_read_only(&root, &q).unwrap();
                            if p.degraded_reasons.iter().any(|r| {
                                matches!(r, DegradedReason::ReaderContentionDeferred { .. })
                            }) {
                                cont.fetch_add(1, Ordering::Relaxed);
                            } else {
                                cursor = p.cursor.next_seq;
                                pages.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }));
                }
            }
            let mut w_handles = Vec::new();
            for w in 0..4usize {
                let path = path.clone();
                w_handles.push(scope.spawn(move || {
                    let c = Connection::open_with_options(
                        &path,
                        dactyl_db::AccessMode::ReadWrite,
                        Duration::from_secs(5),
                    )
                    .unwrap();
                    let mut lats = Vec::with_capacity(100);
                    for i in 0..100usize {
                        let ev = json!({
                            "event_id": format!("bench-w{w}-{i:04}"),
                            "ts": "2026-11-18T00:00:00Z",
                            "event_type": "task.claim",
                            "task_id": format!("task_{w}"),
                            "actor": format!("writer-{w}"),
                            "payload": {"i": i, "pad": "z".repeat(256)}
                        });
                        let t0 = Instant::now();
                        events::append_on_conn(&c, events::TODO, &ev).unwrap();
                        lats.push(t0.elapsed().as_nanos());
                    }
                    lats
                }));
            }
            for h in w_handles {
                writer_ns.extend(h.join().unwrap());
            }
            done.store(true, Ordering::Release);
            for h in r_handles {
                h.join().unwrap();
            }
        });
        (
            writer_ns,
            reader_pages.load(Ordering::Relaxed),
            reader_contended.load(Ordering::Relaxed),
        )
    };

    let (mut solo_writer_ns, _, _) = measure_writers(false);
    let (mut concurrent_writer_ns, reader_pages, reader_contended) = measure_writers(true);

    // --- Benchmark 3: 24-worktree repository enumeration ---
    let (_wt_tmp, wt_repo) = setup_git_repo_with_worktrees(24);
    let t_wt_meta = Instant::now();
    let wt_meta_page = project_worktrees_read_only(
        &wt_repo,
        &WorktreeProjectionQuery {
            after_worktree_path: None,
            include_dirty_summary: false,
            budget: ProjectionBudget::default(),
        },
    )
    .unwrap();
    let wt_meta_ms = t_wt_meta.elapsed().as_millis() as u64;

    let t_wt_dirty = Instant::now();
    let wt_dirty_page = project_worktrees_read_only(
        &wt_repo,
        &WorktreeProjectionQuery {
            after_worktree_path: None,
            include_dirty_summary: true,
            budget: ProjectionBudget::default(),
        },
    )
    .unwrap();
    let wt_dirty_ms = t_wt_dirty.elapsed().as_millis() as u64;

    let report: Value = json!({
        "schema_version": PROJECTION_BUDGET_SCHEMA_VERSION,
        "issue": 1734,
        "event_scale": total_events,
        "subject_query": {
            "unindexed_plan": unindexed_plan,
            "indexed_plan": indexed_plan,
            "unindexed_p50_us": percentile_us(&mut unindexed_subject_ns, 50),
            "unindexed_p95_us": percentile_us(&mut unindexed_subject_ns, 95),
            "indexed_p50_us": percentile_us(&mut indexed_subject_ns, 50),
            "indexed_p95_us": percentile_us(&mut indexed_subject_ns, 95),
        },
        "unbounded_vs_bounded": {
            "unbounded_ms": unbounded_ms,
            "unbounded_response_bytes": unbounded_bytes,
            "bounded_default_p50_us": percentile_us(&mut bounded_page_ns, 50),
            "bounded_default_p95_us": percentile_us(&mut bounded_page_ns, 95),
            "bounded_response_bytes": last_bounded_bytes,
            "bounded_peak_batch_bytes": last_peak_batch_bytes,
            "actors_distinct_p50_us": percentile_us(&mut actors_ns, 50),
        },
        "concurrent_wal_contention": {
            "solo_writers_p50_us": percentile_us(&mut solo_writer_ns, 50),
            "solo_writers_p95_us": percentile_us(&mut solo_writer_ns, 95),
            "solo_writers_p99_us": percentile_us(&mut solo_writer_ns, 99),
            "concurrent_writers_p50_us": percentile_us(&mut concurrent_writer_ns, 50),
            "concurrent_writers_p95_us": percentile_us(&mut concurrent_writer_ns, 95),
            "concurrent_writers_p99_us": percentile_us(&mut concurrent_writer_ns, 99),
            "reader_pages_completed": reader_pages,
            "reader_contention_deferred": reader_contended,
        },
        "worktree_enumeration": {
            "total_worktrees": wt_meta_page.total_discovered,
            "metadata_only_ms": wt_meta_ms,
            "metadata_response_bytes": wt_meta_page.metrics.response_bytes,
            "with_dirty_summary_ms": wt_dirty_ms,
            "with_dirty_summary_status": wt_dirty_page.status,
            "with_dirty_summary_emitted": wt_dirty_page.items.len(),
        }
    });

    if let Ok(out_path) = std::env::var("DECAPOD_PROJECTION_BENCH_REPORT") {
        fs::write(out_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
