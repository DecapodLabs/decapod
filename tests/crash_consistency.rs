use decapod::core::broker::{DbBroker, WriterIncarnation};
use decapod::core::error::DecapodError;
use decapod::core::events;
use serde_json::json;

fn seed_pending(root: &std::path::Path, id: &str, ts: &str) {
    if !events::canonical_db_path(root).exists() {
        decapod::core::todo::initialize_todo_db(root).unwrap();
    }
    events::append(
        root,
        events::BROKER,
        &json!({
            "event_id": id, "request_id": id, "ts": ts, "actor": "crashed-writer",
            "op": "todo.init", "db_id": "decapod.db", "status": "pending",
            "intent_ref": "shared-intent", "correlation_id": "shared-correlation"
        }),
    )
    .unwrap();
}

fn seed_pending_with_incarnation(
    root: &std::path::Path,
    id: &str,
    ts: &str,
    incarnation: &WriterIncarnation,
) {
    if !events::canonical_db_path(root).exists() {
        decapod::core::todo::initialize_todo_db(root).unwrap();
    }
    let mut payload = json!({
        "event_id": id, "request_id": id, "ts": ts, "actor": "crashed-writer",
        "op": "todo.init", "db_id": "decapod.db", "status": "pending",
        "intent_ref": "shared-intent", "correlation_id": "shared-correlation"
    });
    if let Some(pid) = incarnation.writer_pid {
        payload["writer_pid"] = json!(pid);
    }
    if let Some(start_id) = &incarnation.writer_start_id {
        payload["writer_start_id"] = json!(start_id);
    }
    if let Some(boot_id) = &incarnation.writer_boot_id {
        payload["writer_boot_id"] = json!(boot_id);
    }
    if let Some(host_id) = &incarnation.writer_host_id {
        payload["writer_host_id"] = json!(host_id);
    }
    if let Some(pid_ns) = &incarnation.writer_pid_ns {
        payload["writer_pid_ns"] = json!(pid_ns);
    }
    events::append(root, events::BROKER, &payload).unwrap();
}

fn dead_child_incarnation() -> WriterIncarnation {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn exited helper process");
    let pid = child.id();
    let mut incarnation = WriterIncarnation::current();
    incarnation.writer_pid = Some(pid);
    incarnation.writer_start_id = Some("dead-child-start".to_string());
    let _ = child.wait();
    incarnation
}

fn seed_dead_pending(root: &std::path::Path, id: &str, ts: &str) {
    let incarnation = dead_child_incarnation();
    seed_pending_with_incarnation(root, id, ts, &incarnation);
}

#[test]
fn repair_honors_mutation_policy_and_keeps_committed_data() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_dead_pending(root, "orphan", "1Z");
    let conn = decapod::core::db::Connection::open(events::canonical_db_path(root)).unwrap();
    conn.execute_batch("CREATE TABLE committed_data (value TEXT); INSERT INTO committed_data VALUES ('already committed');").unwrap();
    conn.execute("INSERT OR REPLACE INTO risk_zones (id, zone_name, description, requires_approval, required_trust_level, created_at) VALUES ('repair-test', 'control.mutate', 'test gate', 0, 'core', '1Z')", []).unwrap();
    let broker = DbBroker::new(root);
    let error = broker
        .repair("orphan", "Writer stopped", true, "test")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Policy gate denied"), "{error}");
    assert_eq!(broker.verify_replay().unwrap().divergences.len(), 1);
    broker
        .repair("orphan", "Writer stopped", true, "operator")
        .unwrap();
    let value: String = conn
        .query_row("SELECT value FROM committed_data", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, "already committed");
    assert!(broker.verify_replay().unwrap().divergences.is_empty());
}

fn setup_cli_workspace(root_dir: &std::path::Path) -> std::path::PathBuf {
    for args in [
        vec!["init", "-b", "master"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
    ] {
        let out = std::process::Command::new("git")
            .current_dir(root_dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let run_root = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(root_dir)
            .env("XDG_CONFIG_HOME", root_dir.join("config"))
            .env("DECAPOD_AGENT_ID", "operator")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .args(args)
            .output()
            .unwrap()
    };

    let initialized = run_root(&["init"]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    for args in [
        vec!["add", "-A"],
        vec!["commit", "-m", "test: initialize fixture"],
    ] {
        let out = std::process::Command::new("git")
            .current_dir(root_dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let added = run_root(&["todo", "add", "broker repair cli test", "--format", "json"]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added_json: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
    let task_id = added_json["id"].as_str().unwrap().to_string();

    let claimed = run_root(&["todo", "claim", "--id", &task_id, "--format", "json"]);
    assert!(
        claimed.status.success(),
        "{}",
        String::from_utf8_lossy(&claimed.stderr)
    );

    let branch = format!("agent/operator/{task_id}-repair");
    let ensured = run_root(&["workspace", "ensure", "--branch", &branch]);
    assert!(
        ensured.status.success(),
        "{}",
        String::from_utf8_lossy(&ensured.stderr)
    );
    let ensured_json: serde_json::Value = serde_json::from_slice(&ensured.stdout).unwrap();
    std::path::PathBuf::from(ensured_json["worktree_path"].as_str().unwrap())
}

#[test]
fn broker_repair_cli_previews_applies_and_verifies() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = setup_cli_workspace(tmp.path());
    let command = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(&workspace)
            .env("XDG_CONFIG_HOME", tmp.path().join("config"))
            .env("DECAPOD_AGENT_ID", "operator")
            .args(args)
            .output()
            .unwrap()
    };
    let root = tmp.path().join(".decapod/data");
    // 1. Legacy provenance-free pending event reports unknown + human_review_required and refuses silent --apply
    seed_pending(&root, "legacy-orphan", "1Z");
    let verify = command(&["data", "broker", "verify"]);
    assert!(!verify.status.success());
    assert!(
        String::from_utf8_lossy(&verify.stderr).contains("broker repair"),
        "{}{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_json["divergences"][0]["writer_liveness"], "unknown");
    assert_eq!(verify_json["divergences"][0]["human_review_required"], true);

    let legacy_preview = command(&[
        "data",
        "broker",
        "repair",
        "--event-id",
        "legacy-orphan",
        "--reason",
        "Writer stopped",
    ]);
    assert!(legacy_preview.status.success());
    let legacy_preview_json: serde_json::Value =
        serde_json::from_slice(&legacy_preview.stdout).unwrap();
    assert_eq!(legacy_preview_json["status"], "preview");
    assert_eq!(legacy_preview_json["writer_liveness"], "unknown");
    assert_eq!(legacy_preview_json["human_review_required"], true);
    assert_eq!(legacy_preview_json["acknowledgment_required"], true);

    // Silent --apply without --acknowledge-unknown-writer MUST be refused
    let silent_apply = command(&[
        "data",
        "broker",
        "repair",
        "--event-id",
        "legacy-orphan",
        "--reason",
        "Writer stopped",
        "--apply",
    ]);
    assert!(!silent_apply.status.success());

    // Explicit human-reviewed confirmation flag succeeds
    let ack_apply = command(&[
        "data",
        "broker",
        "repair",
        "--event-id",
        "legacy-orphan",
        "--reason",
        "Writer stopped",
        "--apply",
        "--acknowledge-unknown-writer",
    ]);
    assert!(
        ack_apply.status.success(),
        "{}",
        String::from_utf8_lossy(&ack_apply.stderr)
    );
    let ack_apply_json: serde_json::Value = serde_json::from_slice(&ack_apply.stdout).unwrap();
    assert_eq!(ack_apply_json["status"], "abandoned");
    assert_eq!(ack_apply_json["acknowledged_unknown_writer"], true);

    // 2. Verified dead-writer pending event with provenance is repairable without --acknowledge-unknown-writer
    seed_dead_pending(&root, "orphan", &decapod::core::time::now_epoch_z());
    let verify = command(&["data", "broker", "verify"]);
    assert!(!verify.status.success());
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_json["divergences"][0]["writer_liveness"], "stopped");
    assert_eq!(
        verify_json["divergences"][0]["human_review_required"],
        false
    );

    for (args, expected) in [
        (
            vec![
                "data",
                "broker",
                "repair",
                "--event-id",
                "orphan",
                "--reason",
                "Writer stopped",
            ],
            "preview",
        ),
        (
            vec![
                "data",
                "broker",
                "repair",
                "--event-id",
                "orphan",
                "--reason",
                "Writer stopped",
                "--apply",
            ],
            "abandoned",
        ),
    ] {
        let out = command(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["status"], expected);
        assert_eq!(report["original_outcome"], "unknown");
        assert_eq!(report["writer_liveness"], "stopped");
        assert_eq!(report["human_review_required"], false);
    }
    let verify = command(&["data", "broker", "verify"]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn broker_repair_cli_refuses_active_writer_and_allows_pid_reuse() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = setup_cli_workspace(tmp.path());
    let command = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(&workspace)
            .env("XDG_CONFIG_HOME", tmp.path().join("config"))
            .env("DECAPOD_AGENT_ID", "operator")
            .args(args)
            .output()
            .unwrap()
    };

    let root = tmp.path().join(".decapod/data");
    // Spawn a real child process via decapod? We can capture the child's incarnation using
    // our current process's boot/host/ns and a live child `sleep` process, or test via current process!
    // Actually, we can capture current process incarnation via WriterIncarnation::current() (since
    // `cargo test` process itself IS alive during the CLI invocation!).
    let live_incarnation = WriterIncarnation::current();
    seed_pending_with_incarnation(
        &root,
        "live-writer",
        &decapod::core::time::now_epoch_z(),
        &live_incarnation,
    );

    // Same PID as live test runner, but different start_id -> proves PID reuse!
    let mut reused_incarnation = live_incarnation.clone();
    reused_incarnation.writer_start_id = Some("prior-process-start-id".to_string());
    seed_pending_with_incarnation(
        &root,
        "pid-reused",
        &decapod::core::time::now_epoch_z(),
        &reused_incarnation,
    );

    let verify = command(&["data", "broker", "verify"]);
    assert!(!verify.status.success());
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    let live_div = verify_json["divergences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["event_id"] == "live-writer")
        .unwrap();
    let reuse_div = verify_json["divergences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["event_id"] == "pid-reused")
        .unwrap();
    assert_eq!(live_div["writer_liveness"], "active");
    assert_eq!(live_div["human_review_required"], false);
    assert_eq!(reuse_div["writer_liveness"], "stopped");
    assert_eq!(reuse_div["human_review_required"], false);

    // Active live writer MUST be refused even with --acknowledge-unknown-writer
    for extra in [
        &[][..],
        &["--apply"][..],
        &["--apply", "--acknowledge-unknown-writer"][..],
    ] {
        let mut args = vec![
            "data",
            "broker",
            "repair",
            "--event-id",
            "live-writer",
            "--reason",
            "Incorrect claim",
        ];
        args.extend_from_slice(extra);
        assert!(
            !command(&args).status.success(),
            "live writer should be refused for args {args:?}"
        );
    }

    // Reused PID (same PID, different start_id) MUST succeed without --acknowledge-unknown-writer
    let reuse_apply = command(&[
        "data",
        "broker",
        "repair",
        "--event-id",
        "pid-reused",
        "--reason",
        "PID reused after crash",
        "--apply",
    ]);
    assert!(
        reuse_apply.status.success(),
        "{}",
        String::from_utf8_lossy(&reuse_apply.stderr)
    );
    let reuse_json: serde_json::Value = serde_json::from_slice(&reuse_apply.stdout).unwrap();
    assert_eq!(reuse_json["status"], "abandoned");
    assert_eq!(reuse_json["writer_liveness"], "stopped");
}

#[test]
fn repair_preserves_history_and_only_abandons_the_selected_event() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_dead_pending(root, "orphan-one", "1Z");
    seed_dead_pending(root, "orphan-two", "1Z");
    let broker = DbBroker::new(root);
    let before = events::query(root, events::BROKER, usize::MAX).unwrap();
    let preview = broker
        .repair("orphan-one", "Writer exited", false, "test")
        .unwrap();
    assert_eq!(preview.status, "preview");
    assert_eq!(
        before,
        events::query(root, events::BROKER, usize::MAX).unwrap()
    );

    let repaired = broker
        .repair("orphan-one", "Writer exited", true, "test")
        .unwrap();
    assert_eq!(repaired.status, "abandoned");
    assert_eq!(repaired.original_outcome, "unknown");
    let report = broker.verify_replay().unwrap();
    assert_eq!(report.divergences.len(), 1);
    assert_eq!(report.divergences[0].event_id, "orphan-two");
    let audit = events::query(root, events::BROKER, usize::MAX).unwrap();
    assert!(before.iter().all(|original| audit.contains(original)));
    let acknowledgment = audit
        .iter()
        .find(|event| event.payload["status"] == "abandoned")
        .unwrap();
    assert_eq!(acknowledgment.payload["causation_id"], "orphan-one");
    assert_eq!(acknowledgment.payload["reason"], "Writer exited");
    assert_eq!(acknowledgment.payload["original_op"], "todo.init");
    assert_eq!(acknowledgment.payload["original_outcome"], "unknown");
    assert_eq!(acknowledgment.payload["actor"], "test");

    let repeated = broker.repair("orphan-one", "Retry", true, "test").unwrap();
    assert_eq!(repeated.status, "already_abandoned");
    assert_eq!(
        repeated.acknowledgment_event_id,
        repaired.acknowledgment_event_id
    );
    assert_eq!(
        events::query(root, events::BROKER, usize::MAX)
            .unwrap()
            .iter()
            .filter(|event| event.payload["status"] == "abandoned")
            .count(),
        1
    );
    broker
        .repair("orphan-two", "Writer also exited", true, "test")
        .unwrap();
    assert!(broker.verify_replay().unwrap().divergences.is_empty());
}

#[test]
fn repair_refuses_recent_completed_unknown_and_reasonless_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_pending(root, "active", &decapod::core::time::now_epoch_z());
    let broker = DbBroker::new(root);
    for (id, reason) in [
        ("active", "May still run"),
        ("missing", "Exited"),
        ("active", "  "),
    ] {
        assert!(broker.repair(id, reason, true, "test").is_err());
    }
    seed_pending(root, "finished", "1Z");
    events::append(
        root,
        events::BROKER,
        &json!({
            "event_id": "terminal", "ts": "2Z", "actor": "crashed-writer",
            "op": "todo.init", "db_id": "decapod.db", "status": "success",
            "intent_ref": "shared-intent"
        }),
    )
    .unwrap();
    assert!(broker.repair("finished", "Exited", true, "test").is_err());
    assert!(
        !events::query(root, events::BROKER, usize::MAX)
            .unwrap()
            .iter()
            .any(|event| event.payload["status"] == "abandoned")
    );
}

#[test]
fn concurrent_repair_appends_one_acknowledgment() {
    let tmp = tempfile::tempdir().unwrap();
    seed_dead_pending(tmp.path(), "orphan", "1Z");
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    DbBroker::new(tmp.path())
                        .repair("orphan", "Writer stopped", true, "test")
                        .unwrap()
                })
            })
            .collect();
        let mut statuses: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().status)
            .collect();
        statuses.sort();
        assert_eq!(statuses, ["abandoned", "already_abandoned"]);
    });
    assert!(
        DbBroker::new(tmp.path())
            .verify_replay()
            .unwrap()
            .divergences
            .is_empty()
    );
}

#[test]
fn read_completion_audits_and_repairs_share_consistent_lock_order() {
    let tmp = tempfile::tempdir().unwrap();
    seed_dead_pending(tmp.path(), "orphan", "1Z");
    let barrier = std::sync::Barrier::new(3);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            barrier.wait();
            let broker = DbBroker::new(tmp.path());
            let path = events::canonical_db_path(tmp.path());
            for _ in 0..20 {
                let value: i64 = broker
                    .with_conn(&path, "read-audit-regression", None, "todo.get", |conn| {
                        Ok(conn.query_row("SELECT 1", [], |row| row.get(0))?)
                    })
                    .unwrap();
                assert_eq!(value, 1);
            }
        });
        let writers: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    (0..8)
                        .map(|_| {
                            DbBroker::new(tmp.path())
                                .repair("orphan", "Writer stopped", true, "test")
                                .unwrap()
                                .status
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        reader.join().unwrap();
        let statuses: Vec<_> = writers
            .into_iter()
            .flat_map(|writer| writer.join().unwrap())
            .collect();
        assert_eq!(
            statuses
                .iter()
                .filter(|status| *status == "abandoned")
                .count(),
            1
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| *status == "already_abandoned")
                .count(),
            15
        );
    });
    let audit = events::query(tmp.path(), events::BROKER, usize::MAX).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|event| {
                event.payload["status"] == "abandoned" && event.payload["causation_id"] == "orphan"
            })
            .count(),
        1
    );
    assert!(
        audit
            .iter()
            .any(|event| event.payload["event_id"] == "orphan"
                && event.payload["status"] == "pending")
    );
    assert_eq!(
        audit
            .iter()
            .filter(|event| {
                event.payload["actor"] == "read-audit-regression"
                    && event.payload["op"] == "todo.get"
                    && event.payload["status"] == "success"
            })
            .count(),
        20
    );
    assert!(
        DbBroker::new(tmp.path())
            .verify_replay()
            .unwrap()
            .divergences
            .is_empty()
    );
}

#[test]
fn test_demonstrate_crash_divergence_risk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // Broker writes and replay verification share the canonical datastore.
    decapod::core::todo::initialize_todo_db(root).unwrap();
    let db_path = events::canonical_db_path(root);

    // Initialize DB
    {
        let conn = decapod::core::db::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
    }

    let broker = DbBroker::new(root);

    // Simulate a crash by panicking inside the closure.
    let result = std::panic::catch_unwind(|| {
        let _: Result<(), DecapodError> =
            broker.with_conn(&db_path, "test-actor", None, "test.op", |conn| {
                conn.execute("INSERT INTO kv (key, value) VALUES ('k1', 'v1')", [])
                    .unwrap();
                // "Crash"
                panic!("SIMULATED CRASH");
            });
    });

    assert!(result.is_err());

    // Verify DB has the data
    {
        let conn = decapod::core::db::Connection::open(&db_path).unwrap();
        let val: String = conn
            .query_row("SELECT value FROM kv WHERE key = 'k1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(val, "v1");
    }

    // Verify log DOES have the 'pending' event, but NO terminal event
    let report = broker.verify_replay().unwrap();
    assert_eq!(
        report.divergences.len(),
        1,
        "Should detect one divergence from the simulated crash"
    );
    assert_eq!(report.divergences[0].op, "test.op");
    assert_eq!(
        report.divergences[0].reason,
        "Pending event without terminal status (potential crash)"
    );
}
