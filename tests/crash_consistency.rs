use decapod::core::broker::DbBroker;
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

#[test]
fn repair_honors_mutation_policy_and_keeps_committed_data() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_pending(root, "orphan", "1Z");
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

#[test]
fn broker_repair_cli_previews_applies_and_verifies() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .current_dir(tmp.path())
            .args(["init", "-b", "test-broker-repair"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let workspace = tmp.path().join(".decapod/workspaces/repair-test");
    let command = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(if workspace.exists() {
                workspace.as_path()
            } else {
                tmp.path()
            })
            .env("XDG_CONFIG_HOME", tmp.path().join("config"))
            .env("DECAPOD_AGENT_ID", "unknown")
            .args(args)
            .output()
            .unwrap()
    };
    let initialized = command(&["init"]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "test: initialize fixture",
        ],
        vec![
            "worktree",
            "add",
            "-b",
            "agent/test/bugs_01h0000000000000-repair",
            workspace.to_str().unwrap(),
        ],
    ] {
        let output = std::process::Command::new("git")
            .current_dir(tmp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let root = tmp.path().join(".decapod/data");
    seed_pending(&root, "orphan", "1Z");
    let verify = command(&["data", "broker", "verify"]);
    assert!(!verify.status.success());
    assert!(
        String::from_utf8_lossy(&verify.stderr).contains("broker repair"),
        "{}{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
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
    }
    let verify = command(&["data", "broker", "verify"]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn repair_preserves_history_and_only_abandons_the_selected_event() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_pending(root, "orphan-one", "1Z");
    seed_pending(root, "orphan-two", "1Z");
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
    seed_pending(tmp.path(), "orphan", "1Z");
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
fn test_demonstrate_crash_divergence_risk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db_path = root.join("test.db");

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
