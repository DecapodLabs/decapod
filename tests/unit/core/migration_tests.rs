// Moved from src/decapod/core/migration.rs
use super::*;
use tempfile::tempdir;

#[test]
fn proven_consolidation_copies_forward_and_retires_recreated_databases() {
    let root = tempdir().unwrap();
    let data_root = root.path().join("data");
    fs::create_dir_all(&data_root).unwrap();
    let target = Connection::open(data_root.join(schemas::LOCAL_DB_NAME)).unwrap();
    initialize_single_datastore_schema(&target).unwrap();
    drop(target);
    Connection::open(data_root.join(schemas::GOVERNANCE_DB_NAME)).unwrap();
    fs::write(
        data_root.join("watcher.events.jsonl"),
        "{\"event_id\":\"already-imported\",\"event_type\":\"watcher.run\"}\n",
    )
    .unwrap();
    store_applied_migrations(
        root.path(),
        &AppliedMigrationLedger {
            schema_version: "1.0.0".to_string(),
            entries: vec![AppliedMigrationEntry {
                id: "db.consolidate.single_datastore.v001".to_string(),
                sequence: 500,
                scope: "global".to_string(),
                kind: "rust".to_string(),
                script_path: None,
                min_version: "0.89.1".to_string(),
                target_version: "0.89.1".to_string(),
                applied_at: "2026-08-01T00:00:00Z".to_string(),
                applied_by_version: "0.92.0".to_string(),
            }],
        },
    )
    .unwrap();

    reconcile_post_consolidation_artifacts(root.path()).unwrap();

    assert!(!data_root.join(schemas::GOVERNANCE_DB_NAME).exists());
    let target = Connection::open(data_root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let receipt: String = target
        .query_row(
            "SELECT content_hash FROM legacy_event_imports WHERE filename = 'watcher.events.jsonl'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(receipt, "proven-by:db.consolidate.single_datastore.v001");
}
