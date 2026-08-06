// Moved from src/decapod/core/migration.rs
use super::*;
use std::collections::HashSet;
use tempfile::tempdir;

#[test]
fn pending_migration_plan_is_versioned_and_ledger_aware() {
    let migrations = all_migrations();
    let mut applied = HashSet::new();
    applied.insert(migrations[0].id.to_string());

    let pending = plan_pending_migrations(DECAPOD_VERSION, &migrations, &applied).unwrap();

    assert!(
        pending
            .iter()
            .all(|migration| migration.id != migrations[0].id)
    );
    assert!(
        pending
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
}

#[test]
fn migration_ledger_records_application_metadata_without_execution() {
    let migrations = all_migrations();
    let mut ledger = AppliedMigrationLedger {
        schema_version: "1.0.0".to_string(),
        entries: Vec::new(),
    };

    ledger.record(&migrations[0]);

    assert_eq!(ledger.entries.len(), 1);
    assert_eq!(ledger.entries[0].id, migrations[0].id);
    assert_eq!(ledger.entries[0].sequence, migrations[0].sequence);
    assert!(!ledger.entries[0].applied_at.is_empty());
}

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
