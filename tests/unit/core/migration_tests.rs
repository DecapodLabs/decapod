// Moved from src/decapod/core/migration.rs
use super::*;
use std::collections::HashSet;
use tempfile::tempdir;

#[test]
fn empty_legacy_todo_is_preserved_without_reopening_or_changing_journal() {
    let root = tempdir().unwrap();
    check_and_migrate(root.path()).unwrap();
    let data = root.path().join("data");
    crate::core::todo::initialize_todo_db(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    // A real task baseline, including the journal's complete projection.
    conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope) VALUES('feat_empty_legacy','hash:fixture','preserve custody','open','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','.','repo')", []).unwrap();
    drop(conn);
    crate::core::todo::task_journal_checkpoint(&data, None).unwrap();
    let before = crate::core::todo::task_journal_verify(&data).unwrap();
    let legacy = data.join(schemas::TODO_DB_NAME);
    fs::write(&legacy, []).unwrap();
    for other_legacy in [false, true, false] {
        if other_legacy {
            Connection::open(data.join(schemas::GOVERNANCE_DB_NAME)).unwrap();
        }
        check_and_migrate_with_backup_report(root.path(), |_| Ok(())).unwrap();
        assert_eq!(fs::read(&legacy).unwrap(), Vec::<u8>::new());
        assert_eq!(
            crate::core::todo::task_journal_verify(&data).unwrap(),
            before
        );
    }
}

#[test]
fn uncertain_legacy_todo_remains_guarded_before_import_or_removal() {
    for suffix in ["populated", "-wal", "-shm", "-journal"] {
        let root = tempdir().unwrap();
        check_and_migrate(root.path()).unwrap();
        let data = root.path().join("data");
        crate::core::todo::initialize_todo_db(&data).unwrap();
        let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
        events::append_on_conn(
            &conn,
            events::TODO,
            &serde_json::json!({
                "event_id":"guard-fixture", "event_type":"task.journal.checkpoint"
            }),
        )
        .unwrap();
        drop(conn);
        let legacy = data.join(schemas::TODO_DB_NAME);
        fs::write(
            &legacy,
            if suffix == "populated" {
                b"untrusted".as_slice()
            } else {
                b"".as_slice()
            },
        )
        .unwrap();
        if suffix != "populated" {
            fs::write(
                data.join(format!("{}{suffix}", schemas::TODO_DB_NAME)),
                b"evidence",
            )
            .unwrap();
        }
        let before = fs::read(&legacy).unwrap();
        let error = migrate_consolidate_single_datastore(root.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("TASK_JOURNAL_MIGRATION_REQUIRED"),
            "{suffix}: {error}"
        );
        assert_eq!(fs::read(&legacy).unwrap(), before);
    }
}

#[cfg(unix)]
#[test]
fn legacy_todo_symlink_is_never_classified_as_an_empty_placeholder() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("outside"), []).unwrap();
    std::os::unix::fs::symlink(
        root.path().join("outside"),
        root.path().join(schemas::TODO_DB_NAME),
    )
    .unwrap();
    assert!(!inert_legacy_todo_placeholder(root.path()));
    assert!(legacy_database_artifact_exists(
        root.path(),
        schemas::TODO_DB_NAME
    ));
    fs::remove_file(root.path().join("outside")).unwrap();
    assert!(!inert_legacy_todo_placeholder(root.path()));
    assert!(legacy_database_artifact_exists(
        root.path(),
        schemas::TODO_DB_NAME
    ));
}

#[test]
fn event_index_v2_upgrade_records_once_and_preserves_checkpoint_bytes() {
    let root = tempdir().unwrap();
    check_and_migrate(root.path()).unwrap();
    crate::core::todo::initialize_todo_db(&root.path().join("data")).unwrap();
    let path = root.path().join("data").join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(schemas::EVENTS_TABLE_SCHEMA).unwrap();
    let event = serde_json::json!({"event_id":"index-upgrade-proof", "event_type":"task.journal.checkpoint", "task_id":"feat_fixture00000001", "payload":{"task_revision":7,"proof_ref":"sha256:unchanged"}});
    let seq = events::append_on_conn(&conn, events::TODO, &event).unwrap();
    let before: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='index-upgrade-proof'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Reconstruct the immediately preceding applied ledger in this disposable
    // fixture only; historical migration definitions remain unchanged.
    let mut ledger = load_applied_migrations(root.path()).unwrap();
    ledger
        .entries
        .retain(|m| m.id != "events.compact_sequence_index.v002");
    store_applied_migrations(root.path(), &ledger).unwrap();
    let first = check_and_migrate_with_backup_report(root.path(), |_| Ok(())).unwrap();
    assert_eq!(
        first.applied_migrations,
        ["events.compact_sequence_index.v002"]
    );
    let second = check_and_migrate_with_backup_report(root.path(), |_| Ok(())).unwrap();
    assert!(second.applied_migrations.is_empty());
    let after: (u64, String) = conn
        .query_row(
            "SELECT seq,payload FROM events WHERE event_id='index-upgrade-proof'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(after, (seq, before));
    assert!(
        conn.inspect_schema()
            .unwrap()
            .indexes
            .iter()
            .all(|i| i.name != "idx_events_stream_seq")
    );
}

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
    // Replacement must preserve the physical-owner exclusion inode. Removing
    // it while held would let another process elect an owner on a new inode.
    let maintenance = crate::core::storage_lock::StorageLock::acquire(
        &database,
        crate::core::storage_lock::StorageLockMode::Exclusive,
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    use std::os::unix::fs::MetadataExt;
    let lock_inode = fs::metadata(maintenance.path()).unwrap().ino();
    let runtime = data.join("broker-runtime");
    crate::core::fs_permissions::ensure_storage_dir(&runtime).unwrap();
    let runtime_inode = fs::metadata(&runtime).unwrap().ino();
    let backup = create_data_backup(&data).unwrap().unwrap();
    assert!(!backup.join("decapod.db.lock").exists());
    assert!(!backup.join("broker-runtime").exists());
    // A historical backup from an older binary must not replace the lock.
    fs::write(backup.join("decapod.db.lock"), b"historical marker").unwrap();
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
    assert_eq!(fs::metadata(maintenance.path()).unwrap().ino(), lock_inode);
    assert_eq!(fs::metadata(&runtime).unwrap().ino(), runtime_inode);
    drop(maintenance);
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
