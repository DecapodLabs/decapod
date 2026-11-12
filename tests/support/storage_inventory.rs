use decapod::core::{
    dactyl::DactylBridge,
    storage_inventory::{InventoryOptions, inspect},
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn fixture(root: &Path) -> DactylBridge {
    let data = root.join(".decapod/data");
    std::fs::create_dir_all(&data).unwrap();
    // This is a disposable caller-owned fixture, never a dogfooding store.
    let bridge =
        DactylBridge::open_local(data.join("decapod.db"), dactyl_db::AccessMode::ReadWrite)
            .unwrap();
    bridge
        .write(
            "CREATE TABLE tasks(id TEXT PRIMARY KEY,description TEXT,tags TEXT)",
            &[],
        )
        .unwrap();
    bridge
        .write(
            "CREATE TABLE events(event_id TEXT PRIMARY KEY,payload TEXT,event_type TEXT)",
            &[],
        )
        .unwrap();
    std::fs::write(
        root.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n",
    )
    .unwrap();
    bridge
}

fn task_report(report: &Value) -> &Value {
    report["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["table"] == "tasks")
        .unwrap()
}

#[test]
fn inventory_measures_utf8_and_component_reuse_without_exposing_content() {
    let temp = tempfile::tempdir().unwrap();
    let bridge = fixture(temp.path());
    let private = format!("PRIVATE_CUSTOMER_{}", "é🙂".repeat(128));
    for id in ["private-task-a", "private-task-b"] {
        bridge
            .write(
                "INSERT INTO tasks VALUES($1,$2,NULL)",
                &[id.into(), private.clone().into()],
            )
            .unwrap();
        let checkpoint = json!({"schema_version":1,"projection":{
            "task":{"description":private,"scope":"root"},
            "verification":{"proof_plan":"proof","verification_artifacts":null}
        }});
        let event = json!({"event_id":id,"actor":"private-principal","payload":checkpoint});
        bridge
            .write(
                "INSERT INTO events VALUES($1,$2,'task.journal.checkpoint')",
                &[id.into(), event.to_string().into()],
            )
            .unwrap();
    }
    let before = bridge.read("SELECT * FROM tasks ORDER BY id", &[]).unwrap();
    let report = inspect(
        &temp.path().join(".decapod/data"),
        "fixture",
        &InventoryOptions::default(),
    )
    .unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["coverage"], "complete");
    let tasks = task_report(&report);
    assert_eq!(tasks["rows"], 2);
    assert_eq!(
        tasks["fields"]["description"]["logical_bytes"],
        private.len() * 2
    );
    assert_eq!(
        tasks["fields"]["description"]["potential_repeated_bytes"],
        private.len()
    );
    assert_eq!(
        report["component_classes"]["checkpoint_description"]["potential_repeated_bytes"],
        private.len()
    );
    assert!(report["measured_savings_bytes"].is_null());
    assert_eq!(
        report["sampled_format_observations"]["checkpoint_versions"]["1"],
        2
    );
    assert!(
        report["effective_schema_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    let encoded = report.to_string();
    for secret in [
        private.as_str(),
        "private-task-a",
        "private-task-b",
        "private-principal",
    ] {
        assert!(!encoded.contains(secret));
    }
    assert_eq!(
        bridge.read("SELECT * FROM tasks ORDER BY id", &[]).unwrap(),
        before
    );
}

#[test]
fn bounded_coverage_and_missing_metrics_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let bridge = fixture(temp.path());
    bridge
        .write(
            "INSERT INTO tasks VALUES('a','short',NULL),('b','short',NULL)",
            &[],
        )
        .unwrap();
    let data = temp.path().join(".decapod/data");
    let sampled = inspect(
        &data,
        "fixture",
        &InventoryOptions {
            rows_per_table: 1,
            ..InventoryOptions::default()
        },
    )
    .unwrap();
    assert_eq!(sampled["coverage"], "sampled");
    assert_eq!(task_report(&sampled)["rows"], 2);
    assert_eq!(task_report(&sampled)["sample_rows"], 1);
    let limited = inspect(
        &data,
        "fixture",
        &InventoryOptions {
            scan_bytes: 1,
            ..InventoryOptions::default()
        },
    )
    .unwrap();
    assert_eq!(limited["status"], "partial");
    assert_eq!(limited["limitation"], "inspection_budget_exceeded");
    let small_report = inspect(
        &data,
        "fixture",
        &InventoryOptions {
            report_bytes: 4096,
            ..InventoryOptions::default()
        },
    )
    .unwrap();
    assert_eq!(small_report["status"], "partial");
    assert_eq!(
        small_report["limitation"],
        "inventory_report_budget_exceeded"
    );
    assert!(small_report["measurements"].is_null());
    let missing = tempfile::tempdir().unwrap();
    let report = inspect(missing.path(), "missing", &InventoryOptions::default()).unwrap();
    assert_eq!(report["status"], "unavailable");
    assert_eq!(report["files_before"]["main"]["status"], "absent");
    assert!(report["files_before"]["main"]["apparent_bytes"].is_null());
    assert!(!missing.path().join("decapod.db").exists());
}

#[test]
fn ordinary_ddl_literals_and_generated_columns_are_measured() {
    let temp = tempfile::tempdir().unwrap();
    let bridge = fixture(temp.path());
    bridge.write("CREATE TABLE ordinary(id TEXT PRIMARY KEY, note TEXT DEFAULT 'CREATE VIRTUAL TABLE', derived TEXT GENERATED ALWAYS AS (note || '!') VIRTUAL)", &[]).unwrap();
    bridge
        .write("INSERT INTO ordinary(id) VALUES('private-id')", &[])
        .unwrap();
    let report = inspect(
        &temp.path().join(".decapod/data"),
        "fixture",
        &InventoryOptions::default(),
    )
    .unwrap();
    assert_eq!(report["status"], "ok");
    let ordinary = report["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|table| table["table"] == "ordinary")
        .unwrap();
    assert_eq!(ordinary["rows"], 1);
    assert_eq!(
        ordinary["fields"]["note"]["logical_bytes"],
        "CREATE VIRTUAL TABLE".len()
    );
    assert_eq!(
        ordinary["fields"]["derived"]["logical_bytes"],
        "CREATE VIRTUAL TABLE!".len()
    );
}

#[test]
fn effective_schema_digest_covers_triggers_and_views_without_exposing_ddl() {
    let temp = tempfile::tempdir().unwrap();
    let bridge = fixture(temp.path());
    let data = temp.path().join(".decapod/data");
    let before = inspect(&data, "fixture", &InventoryOptions::default()).unwrap();
    bridge.write("CREATE TRIGGER reject_empty BEFORE INSERT ON tasks WHEN length(NEW.description)=0 BEGIN SELECT RAISE(ABORT,'PRIVATE_TRIGGER_TEXT'); END", &[]).unwrap();
    let trigger = inspect(&data, "fixture", &InventoryOptions::default()).unwrap();
    assert_ne!(
        before["effective_schema_digest"],
        trigger["effective_schema_digest"]
    );
    bridge
        .write(
            "CREATE VIEW task_view AS SELECT id,'PRIVATE_VIEW_TEXT' AS note FROM tasks",
            &[],
        )
        .unwrap();
    let view = inspect(&data, "fixture", &InventoryOptions::default()).unwrap();
    assert_eq!(view["status"], "ok");
    assert_ne!(
        trigger["effective_schema_digest"],
        view["effective_schema_digest"]
    );
    let encoded = view.to_string();
    assert!(!encoded.contains("PRIVATE_TRIGGER_TEXT"));
    assert!(!encoded.contains("PRIVATE_VIEW_TEXT"));
    assert_eq!(task_report(&view)["rows"], 0);
}

#[test]
fn cli_and_stdin_rpc_inventory_do_not_initialize_a_missing_store() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    std::fs::write(
        root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n",
    )
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(super::resolve_decapod_bin())
        .current_dir(root.path())
        .env("HOME", home.path())
        .args(["data", "database", "inspect", "--scope", "missing"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["limitation"], "missing_database");
    let mut rpc = Command::new(super::resolve_decapod_bin())
        .current_dir(root.path())
        .env("HOME", home.path())
        .args(["rpc", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    rpc.stdin
        .take()
        .unwrap()
        .write_all(
            br#"{"id":"read-only","op":"data.database.inspect","params":{"scope":"missing"}}"#,
        )
        .unwrap();
    let output = rpc.wait_with_output().unwrap();
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["id"], "read-only");
    assert_eq!(response["success"], false);
    assert_eq!(response["error"]["code"], "INVENTORY_INCOMPLETE");
    assert_eq!(response["receipt"]["touched_paths"], Value::Null);
    let direct = Command::new(super::resolve_decapod_bin())
        .current_dir(root.path())
        .env("HOME", home.path())
        .args([
            "rpc",
            "--op",
            "data.database.inspect",
            "--params",
            r#"{"scope":"missing"}"#,
        ])
        .output()
        .unwrap();
    assert!(!direct.status.success());
    let direct: Value = serde_json::from_slice(&direct.stdout).unwrap();
    assert_eq!(direct["error"]["code"], "INVENTORY_INCOMPLETE");
    assert!(!root.path().join(".decapod/data").exists());
    assert!(!root.path().join(".decapod/managed").exists());
}

#[test]
fn stdin_request_framing_is_bounded_before_store_initialization() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    std::fs::write(
        root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n",
    )
    .unwrap();
    let mut child = Command::new(super::resolve_decapod_bin())
        .current_dir(root.path())
        .args(["rpc", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut bytes = br#"{"op":"data.database.inspect","params":{}}"#.to_vec();
    bytes.resize(32 * 1024 * 1024 + 1, b' ');
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("RPC_REQUEST_BUDGET"));
    assert!(!root.path().join(".decapod/data").exists());
}
