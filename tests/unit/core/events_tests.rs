// Moved from src/decapod/core/events.rs
use super::*;
use tempfile::tempdir;

#[test]
fn legacy_import_is_idempotent_and_preserves_json() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("broker.events.jsonl"),
        "{\"event_id\":\"e1\",\"ts\":\"2026-01-01T00:00:00Z\",\"op\":\"x\"}\n",
    )
    .unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 1);
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
    let payload: String = conn
        .query_row(
            "SELECT payload FROM broker_events WHERE event_id = 'e1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["op"], "x");
}

#[test]
fn canonical_query_boundary_reads_appended_events() {
    let dir = tempdir().unwrap();
    let event = serde_json::json!({
        "event_id": "watcher-1",
        "ts": "2026-01-01T00:00:00Z",
        "event_type": "watcher.run",
        "actor": "agent-a"
    });
    append(dir.path(), WATCHER, &event).unwrap();
    assert!(exists(dir.path(), WATCHER).unwrap());
    let latest = latest(dir.path(), WATCHER).unwrap().unwrap();
    assert_eq!(latest.event_id, "watcher-1");
    assert_eq!(latest.payload, event);
    assert_eq!(actors(dir.path(), WATCHER).unwrap(), vec!["agent-a"]);
}

#[test]
fn migrated_watcher_events_remain_observable_without_legacy_jsonl() {
    let dir = tempdir().unwrap();
    let legacy_path = dir.path().join("watcher.events.jsonl");
    fs::write(
        &legacy_path,
        "{\"event_id\":\"legacy-watch-1\",\"ts\":\"1785620000Z\",\"event_type\":\"watcher.run\",\"actor\":\"watcher\"}\n",
    )
    .unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 1);
    drop(conn);

    fs::remove_file(legacy_path).unwrap();
    let event = latest(dir.path(), WATCHER).unwrap().unwrap();
    assert_eq!(event.event_id, "legacy-watch-1");
    assert_eq!(event.event_type, "watcher.run");
}

#[test]
fn legacy_import_fails_visibly_on_conflicting_event_identity() {
    let dir = tempdir().unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    ensure_tables(&conn).unwrap();
    append_on_conn(
        &conn,
        WATCHER,
        &serde_json::json!({
            "event_id": "conflict-1",
            "ts": "1785620000Z",
            "event_type": "watcher.run",
            "actor": "watcher",
            "status": "canonical"
        }),
    )
    .unwrap();
    fs::write(
        dir.path().join("watcher.events.jsonl"),
        "{\"event_id\":\"conflict-1\",\"ts\":\"1785620000Z\",\"event_type\":\"watcher.run\",\"actor\":\"watcher\",\"status\":\"legacy\"}\n",
    )
    .unwrap();

    let error = import_legacy_jsonl(dir.path(), &conn).expect_err("conflict must fail");
    assert!(error.to_string().contains("LEGACY_EVENT_CONFLICT"));
}

#[test]
fn legacy_import_accepts_equivalent_split_envelope_storage() {
    let dir = tempdir().unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    ensure_tables(&conn).unwrap();
    conn.execute(
        "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "federation-equivalent",
            "2026-08-01T00:00:00Z",
            "node.create",
            "node-1",
            "{\"title\":\"Equivalent\"}",
            "agent"
        ],
    )
    .unwrap();
    fs::write(
        dir.path().join(schemas::FEDERATION_EVENTS_NAME),
        "{\"event_id\":\"federation-equivalent\",\"ts\":\"2026-08-01T00:00:00Z\",\"event_type\":\"node.create\",\"node_id\":\"node-1\",\"payload\":{\"title\":\"Equivalent\"},\"actor\":\"agent\"}\n",
    )
    .unwrap();
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
}

#[test]
fn proven_consolidation_retires_legacy_files_as_runtime_inputs() {
    let dir = tempdir().unwrap();
    let legacy_path = dir.path().join("watcher.events.jsonl");
    fs::write(
        &legacy_path,
        "{\"event_id\":\"retired-watch\",\"event_type\":\"watcher.run\"}\n",
    )
    .unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    ensure_tables(&conn).unwrap();
    append_on_conn(
        &conn,
        WATCHER,
        &serde_json::json!({
            "event_id": "canonical-watch",
            "event_type": "watcher.run"
        }),
    )
    .unwrap();

    assert_eq!(
        mark_previously_consolidated_legacy_inputs(
            dir.path(),
            &conn,
            "db.consolidate.single_datastore.v001"
        )
        .unwrap(),
        1
    );
    fs::write(&legacy_path, "not live evidence anymore\n").unwrap();
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM watcher_events", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
