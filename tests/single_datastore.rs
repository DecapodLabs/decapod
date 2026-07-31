use decapod::core::{db, events, migration, schemas, todo};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn fresh_subsystems_share_one_local_database() {
    let temp = tempdir().unwrap();
    let data = temp.path().join("data");

    todo::initialize_todo_db(&data).unwrap();
    decapod::plugins::health::initialize_health_db(&data).unwrap();
    decapod::plugins::federation::initialize_federation_db(&data).unwrap();
    decapod::plugins::cron::initialize_cron_db(&data).unwrap();
    decapod::plugins::reflex::initialize_reflex_db(&data).unwrap();
    decapod::plugins::lcm::initialize_lcm_db(&data).unwrap();
    decapod::plugins::aptitude::initialize_aptitude_db(&data).unwrap();

    let local = data.join(schemas::LOCAL_DB_NAME);
    assert!(local.is_file());
    for legacy_name in [
        schemas::TODO_DB_NAME,
        schemas::GOVERNANCE_DB_NAME,
        schemas::MEMORY_DB_NAME,
        schemas::AUTOMATION_DB_NAME,
        schemas::LCM_DB_NAME,
    ] {
        assert!(
            !data.join(legacy_name).exists(),
            "unexpected legacy file: {legacy_name}"
        );
    }

    let conn = Connection::open(local).unwrap();
    for table in [
        "meta",
        "tasks",
        "claims",
        "nodes",
        "cron_jobs",
        "reflexes",
        "originals_index",
        "preferences",
        "broker_events",
        "todo_events",
        "federation_events",
        "external_actions_events",
        "traces_events",
        "verification_events",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing table {table}");
    }
}

#[test]
fn legacy_jsonl_is_imported_idempotently_without_new_jsonl_writes() {
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let legacy = data.join(schemas::TODO_EVENTS_NAME);
    let legacy_content = "{\"event_id\":\"legacy-todo-1\",\"ts\":\"2026-01-01T00:00:00Z\",\"event_type\":\"task.add\",\"task_id\":\"feat_legacy\",\"payload\":{\"title\":\"legacy\"},\"actor\":\"migration\"}\n";
    std::fs::write(&legacy, legacy_content).unwrap();

    migration::check_and_migrate(&decapod_root).unwrap();
    migration::check_and_migrate(&decapod_root).unwrap();

    let conn = db::db_connect(&data.join(schemas::LOCAL_DB_NAME).to_string_lossy()).unwrap();
    let (count, seq, payload): (i64, i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(seq), MAX(payload) FROM todo_events WHERE event_id='legacy-todo-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(seq, 1);
    assert!(payload.contains("legacy"));
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), legacy_content);
    assert!(events::table_for_stream(events::TODO).is_some());
}

#[test]
fn legacy_databases_are_copied_into_the_canonical_database() {
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(data.join("operator-notes")).unwrap();
    std::fs::write(
        data.join("operator-notes").join("retention.txt"),
        "preserve this artifact",
    )
    .unwrap();

    let legacy_todo = data.join(schemas::TODO_DB_NAME);
    let conn = Connection::open(&legacy_todo).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta(key, value) VALUES('schema_version', '15');",
    )
    .unwrap();
    conn.execute(schemas::TODO_DB_SCHEMA_TASKS, []).unwrap();
    conn.execute(
        "INSERT INTO tasks(id, hash, title, created_at, updated_at, dir_path, scope)
         VALUES('feat_abcdefghijklmnop', 'abcdef', 'legacy task', '1Z', '1Z', '', 'root')",
        [],
    )
    .unwrap();
    let legacy_governance = Connection::open(data.join(schemas::GOVERNANCE_DB_NAME)).unwrap();
    legacy_governance
        .execute(schemas::HEALTH_DB_SCHEMA_CLAIMS, [])
        .unwrap();
    legacy_governance
        .execute(
            "INSERT INTO claims(id, subject, kind, created_at) VALUES('claim-1', 'legacy', 'test', '1Z')",
            [],
        )
        .unwrap();
    let legacy_memory = Connection::open(data.join(schemas::MEMORY_DB_NAME)).unwrap();
    legacy_memory
        .execute(schemas::MEMORY_DB_SCHEMA_NODES, [])
        .unwrap();
    legacy_memory
        .execute(
            "INSERT INTO nodes(id, node_type, title, created_at, updated_at, dir_path)
             VALUES('node-1', 'observation', 'legacy node', '1Z', '1Z', '')",
            [],
        )
        .unwrap();

    migration::check_and_migrate(&decapod_root).unwrap();

    let local = data.join(schemas::LOCAL_DB_NAME);
    assert!(local.is_file());
    assert!(!legacy_todo.exists());
    assert_eq!(
        std::fs::read_to_string(data.join("operator-notes").join("retention.txt")).unwrap(),
        "preserve this artifact"
    );
    let conn = db::db_connect(&local.to_string_lossy()).unwrap();
    let title: String = conn
        .query_row(
            "SELECT title FROM tasks WHERE id='feat_abcdefghijklmnop'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "legacy task");
    let claim_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM claims WHERE id='claim-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(claim_count, 1);
    let node_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE id='node-1'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(node_count, 1);
    let namespace: String = conn
        .query_row(
            "SELECT namespace FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(namespace, schemas::TODO_META_NAMESPACE);
}

#[test]
fn stale_legacy_sqlite_sidecars_are_migrated_without_touching_unknown_files() {
    let temp = tempdir().unwrap();
    let decapod_root = temp.path().join(".decapod");
    let data = decapod_root.join("data");

    todo::initialize_todo_db(&data).unwrap();
    std::fs::write(data.join("todo.db-wal"), "stale sidecar").unwrap();
    std::fs::write(data.join("operator-note.txt"), "keep").unwrap();

    migration::check_and_migrate_with_backup(&decapod_root, |_| Ok(())).unwrap();

    assert!(!data.join("todo.db-wal").exists());
    assert_eq!(
        std::fs::read_to_string(data.join("operator-note.txt")).unwrap(),
        "keep"
    );
    assert!(data.join(schemas::LOCAL_DB_NAME).is_file());
}
