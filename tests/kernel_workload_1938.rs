//! Repository-shaped kernel benchmarks and nonfunctional regression budgets (#1938, #1630, #1400, #1095).
//!
//! Exercises Decapod's real local SQLite, DbBroker, Todo lease/transition, Event journal,
//! Storage compaction, DB maintenance, Pre-build validation, and Bounded Diagnostic Telemetry
//! subsystems across deterministic fixed-seed Small, Medium, and Large skewed workloads.

use decapod::core::db::{self, Connection};
use decapod::core::db_maintenance;
use decapod::core::events;
use decapod::core::pre_build_validate;
use decapod::core::schemas;
use decapod::core::storage_compaction;
use decapod::core::telemetry::{
    DiagnosticOutcome, LatencyQuantilesUs, ProofFreshness, RawDiagnosticSample,
    TelemetryRetentionPolicy, TelemetryRingBuffer, WorkloadSizeMetrics,
    collect_workspace_diagnostic_telemetry, compute_latency_quantiles_us,
};
use decapod::core::todo::{self, ClaimMode, TodoCommand};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier, Mutex};
use std::time::Instant;
use tempfile::tempdir;

const BENCH_SEED: u64 = 0xDECA_90D4_1938_0001;

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkloadProfile {
    name: &'static str,
    tasks: usize,
    agents: usize,
    worktrees: usize,
    skewed_events: usize,
    hot_bucket_pct: usize,
    proof_records: usize,
    concurrent_workers: usize,
    budgets: RegressionBudgets,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct RegressionBudgets {
    max_claim_renew_p99_us: u64,
    max_cold_read_us: u64,
    max_warm_read_p99_us: u64,
    max_replay_us: u64,
    max_compaction_us: u64,
    max_maintenance_us: u64,
    max_pre_build_warm_us: u64,
    max_bytes_per_event: u64,
    max_write_amplification: f64,
}

const SMALL_PROFILE: WorkloadProfile = WorkloadProfile {
    name: "small",
    tasks: 12,
    agents: 2,
    worktrees: 2,
    skewed_events: 120,
    hot_bucket_pct: 20,
    proof_records: 2,
    concurrent_workers: 2,
    budgets: RegressionBudgets {
        max_claim_renew_p99_us: 2_500_000,
        max_cold_read_us: 1_000_000,
        max_warm_read_p99_us: 500_000,
        max_replay_us: 2_000_000,
        max_compaction_us: 3_000_000,
        max_maintenance_us: 3_000_000,
        max_pre_build_warm_us: 1_000_000,
        max_bytes_per_event: 16_384,
        max_write_amplification: 32.0,
    },
};

const MEDIUM_PROFILE: WorkloadProfile = WorkloadProfile {
    name: "medium",
    tasks: 36,
    agents: 6,
    worktrees: 8,
    skewed_events: 600,
    hot_bucket_pct: 15,
    proof_records: 6,
    concurrent_workers: 4,
    budgets: RegressionBudgets {
        max_claim_renew_p99_us: 3_500_000,
        max_cold_read_us: 1_500_000,
        max_warm_read_p99_us: 750_000,
        max_replay_us: 4_000_000,
        max_compaction_us: 5_000_000,
        max_maintenance_us: 4_000_000,
        max_pre_build_warm_us: 1_500_000,
        max_bytes_per_event: 8_192,
        max_write_amplification: 24.0,
    },
};

const LARGE_PROFILE: WorkloadProfile = WorkloadProfile {
    name: "large",
    tasks: 48,
    agents: 16,
    worktrees: 24,
    skewed_events: 1_800,
    hot_bucket_pct: 10,
    proof_records: 12,
    concurrent_workers: 6,
    budgets: RegressionBudgets {
        max_claim_renew_p99_us: 5_000_000,
        max_cold_read_us: 2_000_000,
        max_warm_read_p99_us: 1_000_000,
        max_replay_us: 8_000_000,
        max_compaction_us: 10_000_000,
        max_maintenance_us: 6_000_000,
        max_pre_build_warm_us: 2_000_000,
        max_bytes_per_event: 6_144,
        max_write_amplification: 20.0,
    },
};

#[derive(Debug, Clone, Serialize)]
struct SqliteMetrics {
    main_db_bytes: u64,
    wal_sidecar_bytes: u64,
    shm_sidecar_bytes: u64,
    page_size: u64,
    page_count: u64,
    freelist_count: u64,
    live_bytes: u64,
    index_count: u64,
    table_count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct KernelWorkloadReport {
    tier: &'static str,
    seed: u64,
    tasks: usize,
    agents: usize,
    worktrees: usize,
    total_events: usize,
    proof_records: usize,
    claim_renew_transition: Option<LatencyQuantilesUs>,
    throughput_ops_per_sec: f64,
    contended_retry_us: u64,
    cold_read_us: u64,
    warm_read: Option<LatencyQuantilesUs>,
    replay_us: u64,
    compaction_us: u64,
    events_compacted: u64,
    maintenance_us: u64,
    pre_build_cold_us: u64,
    pre_build_warm_us: u64,
    before_compaction: SqliteMetrics,
    after_compaction: SqliteMetrics,
    after_reclaim: SqliteMetrics,
    logical_payload_bytes: u64,
    bytes_per_event: u64,
    write_amplification: f64,
    lock_wait_us: Option<LatencyQuantilesUs>,
    process_rss_kib: Option<u64>,
    hosted_cloud_comparison: &'static str,
    lossless_digest_verified: bool,
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn sample_process_rss_kib() -> Option<u64> {
    let pid = std::process::id().to_string();
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

fn measure_sqlite(conn: &Connection, db_path: &Path) -> SqliteMetrics {
    let page_size: u64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))
        .unwrap_or(4096) as u64;
    let page_count: u64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64;
    let freelist_count: u64 = conn
        .query_row("PRAGMA freelist_count", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64;
    let index_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64;
    let table_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64;
    let live_pages = page_count.saturating_sub(freelist_count);
    let wal_path = Path::new(&format!("{}-wal", db_path.display())).to_path_buf();
    let shm_path = Path::new(&format!("{}-shm", db_path.display())).to_path_buf();
    SqliteMetrics {
        main_db_bytes: file_len(db_path),
        wal_sidecar_bytes: file_len(&wal_path),
        shm_sidecar_bytes: file_len(&shm_path),
        page_size,
        page_count,
        freelist_count,
        live_bytes: live_pages.saturating_mul(page_size),
        index_count,
        table_count,
    }
}

fn digest_all_event_streams(conn: &Connection) -> (usize, String) {
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    for stream in [
        events::TODO,
        events::WORKUNIT,
        events::TRACES,
        events::VERIFICATION,
    ] {
        let rows = events::query_all_on_conn(conn, stream).expect("query stream for digest");
        total += rows.len();
        for ev in rows {
            hasher.update(stream.as_bytes());
            hasher.update(b":");
            hasher.update(ev.event_id.as_bytes());
            hasher.update(b":");
            let canonical = serde_json::to_string(&ev.payload).unwrap_or_default();
            hasher.update(canonical.as_bytes());
            hasher.update(b"\n");
        }
    }
    (total, format!("{:x}", hasher.finalize()))
}

fn seed_pre_build_fixture(root: &Path) {
    fs::create_dir_all(root.join(".decapod/managed/specs")).unwrap();
    fs::write(
        root.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n\n[init]\nspecs = true\ndiagram_style = \"mermaid\"\nentrypoints = [\"AGENTS.md\"]\n\n[repo]\nproduct_name = \"bench-fixture\"\n",
    )
    .unwrap();
    fs::write(
        root.join(".decapod/governance.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": "1.0.0",
            "claims": {},
            "sections": {
                "plan": { "title": "bench", "intent": "bench", "state": "APPROVED", "todo_ids": [] },
                "trajectory": { "run_id": "bench", "proof_status": "no_checks_run" }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    for spec in [
        "README.md",
        "INTENT.md",
        "ARCHITECTURE.md",
        "INTERFACES.md",
        "SECURITY.md",
        "OPERATIONS.md",
        "VALIDATION.md",
        "SEMANTICS.md",
    ] {
        fs::write(
            root.join(".decapod/managed/specs").join(spec),
            format!("# {spec}\n\nBench fixture living spec content for {spec}.\n"),
        )
        .unwrap();
    }
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.email", "bench@example.com"][..],
        &["config", "user.name", "Kernel Bench"][..],
        &[
            "add",
            ".decapod/config.toml",
            ".decapod/governance.json",
            ".decapod/managed/specs",
        ][..],
        &["commit", "-m", "init kernel benchmark fixture"][..],
    ] {
        let out = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("git fixture setup");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn run_workload_tier(profile: WorkloadProfile) -> KernelWorkloadReport {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let data_dir = root.join(".decapod/data");
    fs::create_dir_all(&data_dir).unwrap();
    for wt in 0..profile.worktrees {
        fs::create_dir_all(root.join(format!(".decapod/workspaces/wt-{wt:02}"))).unwrap();
    }
    seed_pre_build_fixture(root);
    todo::initialize_todo_db(&data_dir).unwrap();
    decapod::plugins::federation::initialize_federation_db(&data_dir).unwrap();

    let mut rng = SplitMix64(BENCH_SEED ^ (profile.tasks as u64));
    let mut ring = TelemetryRingBuffer::new(TelemetryRetentionPolicy::default());
    let mut task_ids = Vec::with_capacity(profile.tasks);

    // 1. Seed repository-shaped tasks distributed across simulated worktree scopes.
    let mut lifecycle_latencies_us = Vec::new();
    let lifecycle_start = Instant::now();
    for idx in 0..profile.tasks {
        let agent = format!("agent-{:02}", idx % profile.agents);
        let wt = idx % profile.worktrees;
        let t0 = Instant::now();
        let added = todo::add_task(
            &data_dir,
            &TodoCommand::Add {
                title: format!("Kernel benchmark task {idx} [tier:{}]", profile.name),
                description: format!(
                    "Repository-shaped task {idx} payload: {}",
                    "custody-proof-lineage-".repeat(4)
                ),
                tags: format!("tier:{},wt:{wt}", profile.name),
                owner: agent,
                due: None,
                r#ref: format!("ref:1938:{idx}"),
                scope: format!("paths:[\"src/mod_{wt}.rs\"]"),
                dir: Some(root.to_string_lossy().to_string()),
                priority: if idx % 5 == 0 { "high" } else { "medium" }.to_string(),
                depends_on: String::new(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .expect("add task");
        let elapsed = t0.elapsed().as_micros() as u64;
        lifecycle_latencies_us.push(elapsed);
        task_ids.push(added["id"].as_str().expect("task id").to_string());
    }

    // 2. Concurrent independent claim / renew (heartbeat) / release transitions.
    let worker_count = profile.concurrent_workers.min(task_ids.len()).max(1);
    let barrier = Arc::new(Barrier::new(worker_count));
    let worker_latencies = Arc::new(Mutex::new(Vec::<u64>::new()));
    let lock_waits = Arc::new(Mutex::new(Vec::<u64>::new()));

    std::thread::scope(|s| {
        for w in 0..worker_count {
            let my_tasks: Vec<String> = task_ids
                .iter()
                .enumerate()
                .filter(|(i, _)| i % worker_count == w)
                .map(|(_, id)| id.clone())
                .collect();
            let b = Arc::clone(&barrier);
            let wl = Arc::clone(&worker_latencies);
            let lw = Arc::clone(&lock_waits);
            let data_ref = &data_dir;
            s.spawn(move || {
                let agent = format!("worker-{w:02}");
                b.wait();
                for task_id in my_tasks {
                    let t_claim = Instant::now();
                    let _ = todo::claim_task(data_ref, &task_id, &agent, ClaimMode::Exclusive)
                        .expect("worker claim");
                    let claim_us = t_claim.elapsed().as_micros() as u64;

                    let t_renew = Instant::now();
                    let _ = todo::renew_claim_lease(data_ref, &task_id, &agent, Some(900))
                        .expect("worker renew");
                    let renew_us = t_renew.elapsed().as_micros() as u64;

                    let t_rel = Instant::now();
                    let _ = todo::release_task_as_actor(data_ref, &task_id, &agent)
                        .expect("worker release");
                    let rel_us = t_rel.elapsed().as_micros() as u64;

                    wl.lock().unwrap().extend([claim_us, renew_us, rel_us]);
                    lw.lock().unwrap().push(claim_us.min(renew_us));
                }
            });
        }
    });

    lifecycle_latencies_us.extend(worker_latencies.lock().unwrap().iter().copied());
    let total_lifecycle_secs = lifecycle_start.elapsed().as_secs_f64().max(1e-6);
    let total_lifecycle_ops = lifecycle_latencies_us.len();
    let throughput_ops_per_sec = total_lifecycle_ops as f64 / total_lifecycle_secs;

    // 3. Failure / retry cost: rival agent attempts claim on claimed task -> conflict, then retries after release.
    let contended_id = &task_ids[0];
    let _ = todo::claim_task(
        &data_dir,
        contended_id,
        "agent-primary",
        ClaimMode::Exclusive,
    )
    .expect("primary claim");
    let t_retry = Instant::now();
    let conflict = todo::claim_task(&data_dir, contended_id, "agent-rival", ClaimMode::Exclusive)
        .expect("conflict response");
    assert_eq!(conflict["status"].as_str(), Some("conflict"));
    let _ = todo::release_task_as_actor(&data_dir, contended_id, "agent-primary")
        .expect("release primary");
    let retried = todo::claim_task(&data_dir, contended_id, "agent-rival", ClaimMode::Exclusive)
        .expect("retry claim succeeds");
    assert_eq!(retried["status"].as_str(), Some("ok"));
    let contended_retry_us = t_retry.elapsed().as_micros() as u64;
    let _ = todo::release_task_as_actor(&data_dir, contended_id, "agent-rival");

    // 4. Append skewed historical events (mixing legacy uncompacted + compact payloads) and proof records.
    let db_path = data_dir.join(schemas::LOCAL_DB_NAME);
    let hot_count = ((profile.tasks * profile.hot_bucket_pct) / 100).max(1);
    let mut logical_payload_bytes: u64 = 0;

    {
        let conn = db::db_connect(&db_path.to_string_lossy()).expect("write conn");
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        for ev_idx in 0..profile.skewed_events {
            // 70% of events target the hot subset of tasks to model repository skew.
            let target_idx = if rng.gen_range(100) < 70 {
                rng.gen_range(hot_count)
            } else {
                rng.gen_range(profile.tasks)
            };
            let task_id = &task_ids[target_idx];
            let stream = match ev_idx % 3 {
                0 => events::WORKUNIT,
                _ => events::TRACES,
            };
            let padded_note = format!(
                "skewed-event-{ev_idx}-{}",
                "governance-custody-checkpoint-envelope-".repeat(4)
            );
            let event_val = json!({
                "event_id": format!("ev1938-{}-{ev_idx:06}", profile.name),
                "ts": format!("{}Z", 1_795_000_000 + ev_idx as u64),
                "event_type": if stream == events::WORKUNIT { "workunit.heartbeat" } else { "trace.span" },
                "actor": format!("agent-{:02}", ev_idx % profile.agents),
                "task_id": task_id,
                "status": "ok",
                "payload": {
                    "task_id": task_id,
                    "seq_hint": ev_idx,
                    "note": padded_note,
                }
            });
            logical_payload_bytes += serde_json::to_string(&event_val).unwrap().len() as u64;
            events::append_on_conn(&conn, stream, &event_val).expect("append skewed event");
        }
        for p_idx in 0..profile.proof_records {
            let sha = format!("{:040x}", p_idx + 1);
            let proof_ev = json!({
                "event_id": format!("proof1938-{}-{p_idx:04}", profile.name),
                "ts": format!("{}Z", 1_795_010_000 + p_idx as u64),
                "event_type": "validation.completed",
                "actor": "validator",
                "candidate_sha": sha,
                "status": "passed",
                "payload": { "gates_passed": 8, "tier": profile.name }
            });
            logical_payload_bytes += serde_json::to_string(&proof_ev).unwrap().len() as u64;
            events::append_on_conn(&conn, events::VERIFICATION, &proof_ev).expect("append proof");
        }
        // Seed 10 legacy uncompacted JSON rows on `traces` so `compact_historical_events`
        // actively compacts real rows and verifies zero-loss payload reconstruction.
        let mut next_trace_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM events WHERE stream = 'traces'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        for leg_idx in 0..10usize {
            next_trace_seq += 1;
            let ev_id = format!("legacy1938-{}-{leg_idx:02}", profile.name);
            let ts = format!("{}Z", 1_795_020_000 + leg_idx as u64);
            let raw_envelope = json!({
                "event_id": ev_id,
                "ts": ts,
                "event_type": "trace.legacy",
                "actor": "agent-legacy",
                "status": "ok",
                "payload": {
                    "note": "legacy-uncompacted-trace-envelope-".repeat(6),
                    "idx": leg_idx
                }
            })
            .to_string();
            logical_payload_bytes += raw_envelope.len() as u64;
            conn.execute(
                "INSERT INTO events (event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
                 VALUES (?1, ?2, ?3, 'traces', NULL, NULL, 'trace.legacy', ?4, 'agent-legacy')",
                decapod::core::db::params![ev_id, ts, next_trace_seq, raw_envelope],
            )
            .expect("insert legacy uncompacted row");
        }
        conn.execute_batch("COMMIT").unwrap();
    }

    // 5. Cold vs Warm Read / Lookup (tasks + hottest skewed subject journal).
    let hottest_task = &task_ids[0];
    let t_cold = Instant::now();
    {
        let cold_conn = db::db_connect_readonly_bounded(
            &db_path.to_string_lossy(),
            std::time::Duration::from_secs(2),
        )
        .expect("cold readonly conn");
        let _ = events::query_subject_on_conn(
            &cold_conn,
            events::WORKUNIT,
            "task",
            hottest_task,
            10_000,
        )
        .expect("cold subject query");
        let _ = todo::get_task(&data_dir, hottest_task).expect("cold get_task");
    }
    let cold_read_us = t_cold.elapsed().as_micros() as u64;

    let mut warm_latencies = Vec::new();
    let warm_conn = db::db_connect_readonly_bounded(
        &db_path.to_string_lossy(),
        std::time::Duration::from_secs(2),
    )
    .expect("warm readonly conn");
    for i in 0..16 {
        let tid = &task_ids[i % hot_count];
        let tw = Instant::now();
        let _ = events::query_subject_on_conn(&warm_conn, events::WORKUNIT, "task", tid, 10_000)
            .expect("warm subject query");
        let _ = todo::get_task(&data_dir, tid).expect("warm get_task");
        warm_latencies.push(tw.elapsed().as_micros() as u64);
    }
    let _ = todo::list_tasks(&data_dir, None, None, None, None, None).expect("list_tasks");
    drop(warm_conn);

    // 6. Replay + Pre-compaction lossless digest capture.
    let conn = db::db_connect(&db_path.to_string_lossy()).expect("rw conn");
    let before_compaction = measure_sqlite(&conn, &db_path);
    let task_count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();

    let t_replay = Instant::now();
    let (total_events, digest_before) = digest_all_event_streams(&conn);
    let journal_verify = todo::task_journal_verify(&data_dir).expect("journal verify");
    assert_eq!(journal_verify["status"].as_str(), Some("ok"));
    let replay_us = t_replay.elapsed().as_micros() as u64;

    // 7. Compaction + GC + explicit reclaim + Zero Loss Assertion.
    let t_compact = Instant::now();
    let compact_rep = storage_compaction::compact_historical_events(&conn).expect("compact events");
    assert!(
        compact_rep.compacted_rows >= 10,
        "compaction must compact seeded legacy rows (got {})",
        compact_rep.compacted_rows
    );
    let _gc_rep = storage_compaction::gc_unreferenced_components(&conn, false).expect("gc");
    let after_compaction = measure_sqlite(&conn, &db_path);
    let _vac_rep = storage_compaction::reclaim_pages_explicit(&conn).expect("explicit reclaim");
    let after_reclaim = measure_sqlite(&conn, &db_path);
    let compaction_us = t_compact.elapsed().as_micros() as u64;

    let (events_after, digest_after) = digest_all_event_streams(&conn);
    let task_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    assert_eq!(
        task_count_before, task_count_after,
        "compaction must preserve exact task count"
    );
    assert_eq!(
        total_events, events_after,
        "compaction must preserve exact event count"
    );
    assert_eq!(
        digest_before, digest_after,
        "compaction + GC + reclaim must preserve exact reconstructed event payload digest"
    );

    // 8. Maintenance probes (inspect + verify + passive WAL checkpoint).
    let policy = db_maintenance::resolve_canonical_policy(root).expect("resolve canonical policy");
    let t_maint = Instant::now();
    let (inspect_val, _) = db_maintenance::run_maintain_inspect(&policy).expect("maintain inspect");
    assert_eq!(inspect_val["status"].as_str(), Some("ok"));
    let (verify_val, _) = db_maintenance::run_maintain_verify(&policy).expect("maintain verify");
    assert_eq!(verify_val["integrity_check"].as_str(), Some("ok"));
    let (wal_val, _) =
        db_maintenance::run_maintain_wal_checkpoint(&policy, 500).expect("maintain wal");
    assert_eq!(wal_val["status"].as_str(), Some("ok"));
    let maintenance_us = t_maint.elapsed().as_micros() as u64;

    // 9. Pre-build validation timing (Cold vs Warm).
    let t_pb_cold = Instant::now();
    let pb_cold = pre_build_validate::evaluate(root);
    let pre_build_cold_us = t_pb_cold.elapsed().as_micros() as u64;
    assert!(
        pb_cold.pass_count > 0,
        "pre_build_validate should evaluate checks"
    );
    let t_pb_warm = Instant::now();
    let _pb_warm = pre_build_validate::evaluate(root);
    let pre_build_warm_us = t_pb_warm.elapsed().as_micros() as u64;

    // 10. Feed bounded telemetry collector & verify diagnostic isolation.
    for (idx, lat) in lifecycle_latencies_us.iter().enumerate() {
        ring.record(
            RawDiagnosticSample {
                epoch_secs: 1_795_000_000 + idx as u64,
                stage: "kernel_bench".to_string(),
                operation: "todo.lifecycle".to_string(),
                outcome: DiagnosticOutcome::Succeeded,
                proof_freshness: ProofFreshness::Fresh,
                queue_wait_us: Some(1),
                execution_us: Some(*lat),
                lock_wait_us: None,
                lock_contended: false,
                repo_identifier: None,
            },
            1_795_000_500,
        );
    }
    let _diag_ring = ring.report(
        WorkloadSizeMetrics {
            tasks: Some(profile.tasks as u64),
            active_agents: Some(profile.agents as u64),
            events: Some(total_events as u64),
            worktrees: Some(profile.worktrees as u64),
            sqlite_bytes: Some(after_reclaim.main_db_bytes),
            wal_bytes: Some(after_reclaim.wal_sidecar_bytes),
        },
        None,
    );
    let ws_diag = collect_workspace_diagnostic_telemetry(
        root,
        1_795_020_000,
        TelemetryRetentionPolicy::default(),
    );
    assert!(ws_diag.read_only);
    assert!(!ws_diag.authoritative);
    assert!(!ws_diag.can_grant_authority);
    assert!(ws_diag.repo_digest.is_none());
    assert_eq!(ws_diag.workload.tasks, Some(profile.tasks as u64));
    assert!(
        ws_diag.workload.events.unwrap_or(0) >= total_events as u64,
        "total events table count ({:?}) must include all measured stream events ({total_events})",
        ws_diag.workload.events
    );

    let bytes_per_event = after_reclaim.main_db_bytes / total_events.max(1) as u64;
    let total_written = before_compaction
        .main_db_bytes
        .saturating_add(before_compaction.wal_sidecar_bytes);
    let write_amplification = total_written as f64 / logical_payload_bytes.max(1) as f64;

    let claim_renew_transition = compute_latency_quantiles_us(&lifecycle_latencies_us);
    let warm_read = compute_latency_quantiles_us(&warm_latencies);
    let lock_wait_us = compute_latency_quantiles_us(&lock_waits.lock().unwrap());

    // 11. Assert compliance with reviewed regression budgets.
    let b = profile.budgets;
    let p99_lifecycle = claim_renew_transition.map(|q| q.p99_us).unwrap_or(0);
    let p99_warm = warm_read.map(|q| q.p99_us).unwrap_or(0);
    assert!(
        p99_lifecycle <= b.max_claim_renew_p99_us,
        "{} lifecycle p99 {}us exceeded budget {}us",
        profile.name,
        p99_lifecycle,
        b.max_claim_renew_p99_us
    );
    assert!(
        cold_read_us <= b.max_cold_read_us,
        "{} cold_read {}us exceeded budget {}us",
        profile.name,
        cold_read_us,
        b.max_cold_read_us
    );
    assert!(
        p99_warm <= b.max_warm_read_p99_us,
        "{} warm_read p99 {}us exceeded budget {}us",
        profile.name,
        p99_warm,
        b.max_warm_read_p99_us
    );
    assert!(
        replay_us <= b.max_replay_us,
        "{} replay {}us exceeded budget {}us",
        profile.name,
        replay_us,
        b.max_replay_us
    );
    assert!(
        compaction_us <= b.max_compaction_us,
        "{} compaction {}us exceeded budget {}us",
        profile.name,
        compaction_us,
        b.max_compaction_us
    );
    assert!(
        maintenance_us <= b.max_maintenance_us,
        "{} maintenance {}us exceeded budget {}us",
        profile.name,
        maintenance_us,
        b.max_maintenance_us
    );
    assert!(
        pre_build_warm_us <= b.max_pre_build_warm_us,
        "{} pre_build_warm {}us exceeded budget {}us",
        profile.name,
        pre_build_warm_us,
        b.max_pre_build_warm_us
    );
    assert!(
        bytes_per_event <= b.max_bytes_per_event,
        "{} bytes_per_event {} exceeded budget {}",
        profile.name,
        bytes_per_event,
        b.max_bytes_per_event
    );
    assert!(
        write_amplification <= b.max_write_amplification,
        "{} write_amplification {:.2} exceeded budget {:.2}",
        profile.name,
        write_amplification,
        b.max_write_amplification
    );

    KernelWorkloadReport {
        tier: profile.name,
        seed: BENCH_SEED,
        tasks: profile.tasks,
        agents: profile.agents,
        worktrees: profile.worktrees,
        total_events,
        proof_records: profile.proof_records,
        claim_renew_transition,
        throughput_ops_per_sec,
        contended_retry_us,
        cold_read_us,
        warm_read,
        replay_us,
        compaction_us,
        events_compacted: compact_rep.compacted_rows,
        maintenance_us,
        pre_build_cold_us,
        pre_build_warm_us,
        before_compaction,
        after_compaction,
        after_reclaim,
        logical_payload_bytes,
        bytes_per_event,
        write_amplification,
        lock_wait_us,
        process_rss_kib: sample_process_rss_kib(),
        hosted_cloud_comparison: "unavailable_offline_no_paid_infra_see_propodus_230_dactyl_144",
        lossless_digest_verified: true,
    }
}

#[test]
fn kernel_workload_1938_small_medium_large_meet_budgets_and_preserve_history() {
    let small = run_workload_tier(SMALL_PROFILE);
    let medium = run_workload_tier(MEDIUM_PROFILE);
    let large = run_workload_tier(LARGE_PROFILE);

    assert!(large.total_events > medium.total_events);
    assert!(medium.total_events > small.total_events);
    assert!(small.lossless_digest_verified);
    assert!(medium.lossless_digest_verified);
    assert!(large.lossless_digest_verified);

    let summary = json!({
        "schema_version": "decapod.kernel-workload-baseline.v1",
        "issue": "#1938",
        "tiers": [small, medium, large],
    });
    println!(
        "KERNEL_WORKLOAD_1938_REPORT={}",
        serde_json::to_string(&summary).unwrap()
    );
}
