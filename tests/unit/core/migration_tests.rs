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
fn migration_report_warns_once_after_a_version_transition() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("managed/migrations")).unwrap();
    fs::write(
        root.path().join("managed/version_counter.json"),
        serde_json::json!({
            "schema_version": "1.0.0",
            "version_count": 3,
            "initialized_with_version": "0.95.0",
            "last_seen_version": "0.96.13",
            "updated_at": "2026-08-01T00:00:00Z"
        })
        .to_string(),
    )
    .unwrap();

    let mut ledger = AppliedMigrationLedger {
        schema_version: "1.0.0".to_string(),
        entries: Vec::new(),
    };
    for migration in all_migrations() {
        ledger.record(&migration);
    }
    store_applied_migrations(root.path(), &ledger).unwrap();

    let first = check_and_migrate_with_backup_report(root.path(), |_| Ok(())).unwrap();
    assert_eq!(first.previous_version.as_deref(), Some("0.96.13"));
    assert!(first.version_changed);
    assert!(first.applied_migrations.is_empty());
    assert!(first.agent_instruction().is_some());

    let second = check_and_migrate_with_backup_report(root.path(), |_| Ok(())).unwrap();
    assert!(!second.version_changed);
    assert!(second.agent_instruction().is_none());
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

#[cfg(unix)]
#[test]
fn backup_restore_and_ledgers_keep_safe_modes_under_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("DECAPOD_MIGRATION_PERMISSION_CHILD").is_none() {
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "migration-child"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "core::migration::tests::backup_restore_and_ledgers_keep_safe_modes_under_permissive_umask", "--nocapture", "--test-threads=1"])
            .env("DECAPOD_MIGRATION_PERMISSION_CHILD", "1")
            .env_remove(crate::core::fs_permissions::SHARED_STORAGE_ENV)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let data = root.path().join("data");
    crate::core::fs_permissions::ensure_storage_dir(&data).unwrap();
    let database = data.join("decapod.db");
    let conn = db::db_connect(database.to_str().unwrap()).unwrap();
    conn.execute_batch(
        "CREATE TABLE retained (value TEXT); INSERT INTO retained VALUES ('original')",
    )
    .unwrap();
    drop(conn);
    let backup = create_data_backup(&data).unwrap().unwrap();
    assert_eq!(
        fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(backup.join("decapod.db"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    restore_data_backup(&data, &backup).unwrap();
    assert_eq!(
        fs::metadata(&database).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let conn = db::db_connect(database.to_str().unwrap()).unwrap();
    let value: String = conn
        .query_row("SELECT value FROM retained", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, "original");
    drop(conn);
    let ledger = AppliedMigrationLedger {
        schema_version: "1.0.0".to_string(),
        entries: vec![],
    };
    store_applied_migrations(root.path(), &ledger).unwrap();
    store_applied_migrations(root.path(), &ledger).unwrap();
    touch_generated_version_counter(root.path()).unwrap();
    touch_generated_migration_catalog(root.path(), &all_migrations()).unwrap();
    for name in [
        GENERATED_APPLIED_MIGRATIONS,
        GENERATED_VERSION_COUNTER,
        GENERATED_MIGRATION_CATALOG,
    ] {
        assert_eq!(
            fs::metadata(root.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
