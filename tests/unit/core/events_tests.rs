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
            "SELECT payload FROM events WHERE stream = 'broker' AND event_id = 'e1'",
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
    // Pre-seed unified events with an equivalent payload shape (split envelope style).
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES(?1, ?2, 1, 'federation', 'node', ?3, ?4, ?5, ?6)",
        params![
            "federation-equivalent",
            "2026-08-01T00:00:00Z",
            "node-1",
            "node.create",
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
fn federation_legacy_import_stores_inner_payload_only() {
    let dir = tempdir().unwrap();
    let node_id = "F_01TESTNODELEGACY001";
    let event_id = "01TESTEVENTLEGACY001";
    let line = serde_json::json!({
        "actor": "decapod",
        "event_id": event_id,
        "event_type": "node.create",
        "node_id": node_id,
        "payload": {
            "node_type": "commitment",
            "title": "Task: legacy import",
            "body": "body",
            "sources": ["event:code_01test"],
            "priority": "notable",
            "scope": "repo",
            "confidence": "agent_inferred",
            "tags": ""
        },
        "status": "success",
        "ts": "1779198477Z"
    });
    fs::write(
        dir.path().join(schemas::FEDERATION_EVENTS_NAME),
        format!("{line}\n"),
    )
    .unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 1);

    let (stored_type, stored_subject, payload_raw): (String, Option<String>, String) = conn
        .query_row(
            "SELECT event_type, subject_id, payload FROM events
             WHERE stream = 'federation' AND event_id = ?1",
            [event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored_type, "node.create");
    assert_eq!(stored_subject.as_deref(), Some(node_id));
    let payload: Value = serde_json::from_str(&payload_raw).unwrap();
    assert_eq!(payload["node_type"], "commitment");
    assert_eq!(payload["title"], "Task: legacy import");
    // Must not re-store the envelope.
    assert!(payload.get("event_type").is_none());
    assert!(payload.get("event_id").is_none());
    assert!(payload.get("payload").is_none());

    // Bookkeeping recorded.
    let count: i64 = conn
        .query_row(
            "SELECT record_count FROM legacy_event_imports WHERE filename = ?1",
            [schemas::FEDERATION_EVENTS_NAME],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    // Idempotent re-import.
    assert_eq!(import_legacy_jsonl(dir.path(), &conn).unwrap(), 0);
}

#[test]
fn federation_legacy_import_rejects_non_object_payload() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(schemas::FEDERATION_EVENTS_NAME),
        format!(
            "{}\n",
            serde_json::json!({
                "event_id": "bad-payload-1",
                "event_type": "edge.add",
                "node_id": "F_1",
                "payload": "not-an-object",
                "actor": "decapod",
                "ts": "1779198477Z"
            })
        ),
    )
    .unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    let err = import_legacy_jsonl(dir.path(), &conn).expect_err("non-object payload must fail");
    assert!(
        err.to_string().contains("LEGACY_EVENT_PAYLOAD")
            || err.to_string().contains("not a JSON object")
            || err.to_string().contains("payload"),
        "unexpected error: {err}"
    );
    // File must not be marked imported; no partial rows.
    let imported: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM legacy_event_imports WHERE filename = ?1",
            [schemas::FEDERATION_EVENTS_NAME],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(imported, 0);
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'federation'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(events, 0);
}

#[test]
fn repair_unwraps_double_wrapped_payloads_transactionally_and_idempotently() {
    let dir = tempdir().unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    ensure_tables(&conn).unwrap();

    let wrapped = serde_json::json!({
        "actor": "decapod",
        "event_id": "e-wrap-1",
        "event_type": "node.create",
        "node_id": "F_wrap_1",
        "payload": {
            "node_type": "commitment",
            "title": "Wrapped title",
            "body": "b",
            "sources": ["event:x"]
        },
        "status": "success",
        "ts": "1779198477Z"
    });
    let native = serde_json::json!({
        "node_type": "lesson",
        "title": "Native title",
        "body": "",
        "sources": []
    });
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES(?1, ?2, 1, 'federation', 'node', ?3, 'node.create', ?4, 'decapod')",
        params![
            "e-wrap-1",
            "1779198477Z",
            "F_wrap_1",
            serde_json::to_string(&wrapped).unwrap()
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES(?1, ?2, 2, 'federation', 'node', ?3, 'node.create', ?4, 'decapod')",
        params![
            "e-native-1",
            "1779198478Z",
            "F_native_1",
            serde_json::to_string(&native).unwrap()
        ],
    )
    .unwrap();

    let report = repair_double_wrapped_federation_payloads(&conn).unwrap();
    assert_eq!(report.candidates, 1);
    assert_eq!(report.normalized, 1);

    let repaired: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'e-wrap-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let repaired_v: Value = serde_json::from_str(&repaired).unwrap();
    assert_eq!(repaired_v["node_type"], "commitment");
    assert_eq!(repaired_v["title"], "Wrapped title");
    assert!(repaired_v.get("event_type").is_none());

    let native_raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'e-native-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&native_raw).unwrap(), native);

    // Metadata columns unchanged.
    let meta: (String, String, Option<String>, String) = conn
        .query_row(
            "SELECT event_type, actor, subject_id, ts FROM events WHERE event_id = 'e-wrap-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(meta.0, "node.create");
    assert_eq!(meta.1, "decapod");
    assert_eq!(meta.2.as_deref(), Some("F_wrap_1"));
    assert_eq!(meta.3, "1779198477Z");

    // Idempotent second pass.
    let report2 = repair_double_wrapped_federation_payloads(&conn).unwrap();
    assert_eq!(report2.candidates, 0);
    assert_eq!(report2.normalized, 0);
}

#[test]
fn repair_aborts_on_type_mismatch_without_partial_changes() {
    let dir = tempdir().unwrap();
    let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
    ensure_tables(&conn).unwrap();

    let good = serde_json::json!({
        "actor": "decapod",
        "event_id": "e-good",
        "event_type": "node.create",
        "node_id": "F_good",
        "payload": {"node_type": "lesson", "title": "Good"},
        "ts": "1779198477Z"
    });
    let bad = serde_json::json!({
        "actor": "decapod",
        "event_id": "e-bad",
        "event_type": "edge.add",
        "node_id": "F_bad",
        "payload": {"edge_id": "FE_1", "source_id": "A", "target_id": "B", "edge_type": "relates_to"},
        "ts": "1779198478Z"
    });
    // Store bad with outer event_type disagreeing with row event_type.
    let bad_wrapped = {
        let mut v = bad.clone();
        v["event_type"] = Value::String("node.create".into());
        v
    };
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES('e-good', '1779198477Z', 1, 'federation', 'node', 'F_good', 'node.create', ?1, 'decapod')",
        params![serde_json::to_string(&good).unwrap()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES('e-bad', '1779198478Z', 2, 'federation', 'node', 'F_bad', 'edge.add', ?1, 'decapod')",
        params![serde_json::to_string(&bad_wrapped).unwrap()],
    )
    .unwrap();

    let err =
        repair_double_wrapped_federation_payloads(&conn).expect_err("type mismatch must abort");
    assert!(
        err.to_string().contains("LEGACY_EVENT_PAYLOAD"),
        "unexpected: {err}"
    );

    // Good candidate must remain wrapped (transaction rolled back).
    let good_raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'e-good'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(looks_like_event_envelope(
        &serde_json::from_str(&good_raw).unwrap()
    ));
    let bad_raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id = 'e-bad'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(looks_like_event_envelope(
        &serde_json::from_str(&bad_raw).unwrap()
    ));
}

#[test]
fn normalize_event_payload_accepts_canonical_and_legacy_shapes() {
    let canonical = serde_json::json!({"node_type": "lesson", "title": "T"});
    let out = normalize_event_payload(
        "e1",
        "node.create",
        Some("F_1"),
        "decapod",
        "1779198477Z",
        &canonical,
    )
    .unwrap();
    assert_eq!(out, canonical);

    let wrapped = serde_json::json!({
        "event_id": "e1",
        "event_type": "node.create",
        "node_id": "F_1",
        "actor": "decapod",
        "ts": "1779198477Z",
        "payload": {"node_type": "lesson", "title": "T"}
    });
    let out = normalize_event_payload(
        "e1",
        "node.create",
        Some("F_1"),
        "decapod",
        "1779198477Z",
        &wrapped,
    )
    .unwrap();
    assert_eq!(out["title"], "T");

    let mismatched = serde_json::json!({
        "event_type": "edge.add",
        "payload": {"edge_id": "x"}
    });
    let err = normalize_event_payload(
        "e1",
        "node.create",
        None,
        "decapod",
        "1779198477Z",
        &mismatched,
    )
    .unwrap_err();
    assert!(err.to_string().contains("LEGACY_EVENT_PAYLOAD"));
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
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'watcher'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
