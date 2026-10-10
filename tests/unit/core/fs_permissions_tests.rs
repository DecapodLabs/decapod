use super::*;

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
#[test]
fn permissive_umask_flows_are_confined() {
    if std::env::var_os("DECAPOD_PERMISSION_CHILD").is_none() {
        // Only the new child shell changes umask. The parallel Rust harness
        // and every other test retain their original process-global state.
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "permissions-child"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::fs_permissions::tests::permissive_umask_flows_are_confined",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("DECAPOD_PERMISSION_CHILD", "1")
            .env_remove(SHARED_STORAGE_ENV)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(
        root.path(),
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let data = root.path().join(".decapod/data");
    let database = data.join("decapod.db");
    let connection = crate::core::db::db_connect(database.to_str().unwrap()).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE example (value TEXT); INSERT INTO example VALUES ('private');")
        .unwrap();
    assert_eq!(mode(&data), 0o700);
    assert_eq!(mode(data.parent().unwrap()), 0o700);
    assert_eq!(mode(&database), 0o600);
    assert_eq!(
        mode(&crate::core::storage_lock::lock_path(&database)),
        0o600
    );
    for suffix in ["-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", database.display()));
        assert!(
            path.is_file(),
            "expected live SQLite sidecar: {}",
            path.display()
        );
        assert_eq!(mode(&path), 0o600);
    }
    drop(connection);
    let rollback = data.join("rollback.db");
    let connection = crate::core::db::db_connect(rollback.to_str().unwrap()).unwrap();
    connection.execute_batch("PRAGMA journal_mode = DELETE; CREATE TABLE rollback_example (value TEXT); BEGIN IMMEDIATE; INSERT INTO rollback_example VALUES ('private')").unwrap();
    assert_eq!(mode(&data.join("rollback.db-journal")), 0o600);
    connection.execute_batch("ROLLBACK").unwrap();
    drop(connection);
    let artifact = root.path().join(".decapod/governance/result.json");
    crate::core::atomic::write_atomic(&artifact, b"first").unwrap();
    crate::core::atomic::write_atomic(&artifact, b"replacement").unwrap();
    assert_eq!(mode(&artifact), 0o600);
    assert_eq!(fs::read(&artifact).unwrap(), b"replacement");
    assert_eq!(fs::read_dir(artifact.parent().unwrap()).unwrap().count(), 1);

    let bridge =
        crate::core::dactyl::DactylBridge::open_local(&database, dactyl_db::AccessMode::ReadWrite)
            .unwrap();
    let backup = root.path().join("backups/snapshot.db");
    bridge.backup(&backup).unwrap();
    let snapshot = fs::read(&backup).unwrap();
    assert!(bridge.backup(&backup).is_err());
    assert_eq!(fs::read(&backup).unwrap(), snapshot);
    assert_eq!(fs::read_dir(backup.parent().unwrap()).unwrap().count(), 1);
    assert_eq!(mode(&backup), 0o600);
    assert_eq!(mode(backup.parent().unwrap()), 0o700);
    drop(bridge);
    let mut bridge =
        crate::core::dactyl::DactylBridge::open_local(&database, dactyl_db::AccessMode::ReadWrite)
            .unwrap();
    let archive = data.join("original.db");
    let recovery = bridge.recover_from_dump_reload(&archive);
    if cfg!(any(target_os = "linux", target_os = "android")) {
        recovery.unwrap();
        assert_eq!(mode(&database), 0o600);
        assert_eq!(mode(&archive), 0o600);
    } else {
        assert!(
            recovery
                .unwrap_err()
                .to_string()
                .contains("secure_recovery_unsupported")
        );
        assert!(!archive.exists());
        assert_eq!(mode(&database), 0o600);
    }
}

#[cfg(unix)]
#[test]
fn unsafe_existing_paths_are_rejected_without_repair() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(
        root.path(),
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let path = root.path().join("state.json");
    fs::write(&path, b"original").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    let error = crate::core::atomic::write_atomic(&path, b"new").unwrap_err();
    assert!(error.to_string().contains("STORAGE_UNSAFE_PERMISSIONS"));
    assert_eq!(mode(&path), 0o666);
    assert_eq!(fs::read(&path).unwrap(), b"original");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = root.path().join("link");
    symlink(&path, &link).unwrap();
    assert!(open_private_file(&link, OpenOptions::new().write(true)).is_err());
    let hard = root.path().join("hard");
    fs::hard_link(&path, &hard).unwrap();
    assert!(check_file(&path, false).is_err());
    let unsafe_parent = root.path().join("unsafe");
    fs::create_dir(&unsafe_parent).unwrap();
    fs::set_permissions(&unsafe_parent, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(ensure_private_dir(&unsafe_parent.join("child")).is_err());
    assert!(!unsafe_parent.join("child").exists());
    let database = root.path().join("database.db");
    drop(crate::core::db::db_connect(database.to_str().unwrap()).unwrap());
    let journal = root.path().join("database.db-journal");
    fs::write(&journal, b"untrusted-sidecar").unwrap();
    fs::set_permissions(&journal, fs::Permissions::from_mode(0o666)).unwrap();
    let error = crate::core::db::db_connect(database.to_str().unwrap()).unwrap_err();
    assert!(error.to_string().contains("database.db-journal"));
    assert_eq!(fs::read(&journal).unwrap(), b"untrusted-sidecar");
}

#[cfg(unix)]
#[test]
fn shared_store_requires_explicit_opt_in_and_never_accepts_world_write() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("DECAPOD_SHARED_PERMISSION_CHILD").is_none() {
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "shared-child"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "core::fs_permissions::tests::shared_store_requires_explicit_opt_in_and_never_accepts_world_write", "--nocapture", "--test-threads=1"])
            .env("DECAPOD_SHARED_PERMISSION_CHILD", "1")
            .env(SHARED_STORAGE_ENV, "1")
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(
        root.path(),
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let data = root.path().join("shared");
    let database = data.join("shared.db");
    let connection = crate::core::db::db_connect(database.to_str().unwrap()).unwrap();
    connection
        .execute_batch("CREATE TABLE shared (id INTEGER); INSERT INTO shared VALUES (7)")
        .unwrap();
    assert_eq!(mode(&data), 0o770);
    assert_eq!(mode(&database), 0o660);
    assert_eq!(
        mode(&crate::core::storage_lock::lock_path(&database)),
        0o660
    );
    assert!(check_file(&database, false).is_err());
    assert!(check_file(&database, true).is_ok());
    assert!(
        open_private_file(
            &data.join("credential"),
            OpenOptions::new().create_new(true).write(true)
        )
        .is_err()
    );
    let reader = crate::core::db::db_connect_read_pooled(database.to_str().unwrap(), 1).unwrap();
    let id: i64 = reader
        .query_row("SELECT id FROM shared", [], |row| row.get(0))
        .unwrap();
    assert_eq!(id, 7);
    drop(reader);
    connection
        .execute_batch("UPDATE shared SET id = 8; DELETE FROM shared WHERE id = 8")
        .unwrap();
    drop(connection);
    let mut bridge =
        crate::core::dactyl::DactylBridge::open_local(&database, dactyl_db::AccessMode::ReadWrite)
            .unwrap();
    let before = fs::read(&database).unwrap();
    let archive = data.join("archive.db");
    let recovery = bridge.recover_from_dump_reload(&archive);
    if cfg!(any(target_os = "linux", target_os = "android")) {
        recovery.unwrap();
        assert_eq!(fs::read(&archive).unwrap(), before);
        assert_eq!(mode(&database), 0o660);
        assert_eq!(mode(&archive), 0o660);
    } else {
        assert!(
            recovery
                .unwrap_err()
                .to_string()
                .contains("secure_recovery_unsupported")
        );
        assert!(!archive.exists());
        assert_eq!(fs::read(&database).unwrap(), before);
    }
    assert!(!fs::read_dir(&data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("dactyl-recovery")
    }));
    drop(bridge);
    fs::set_permissions(&database, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(crate::core::db::db_connect(database.to_str().unwrap()).is_err());
    assert_eq!(mode(&database), 0o666);
}

#[cfg(unix)]
#[test]
fn shared_bootstrap_keeps_managed_configuration_private() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("DECAPOD_SHARED_BOOTSTRAP_CHILD").is_none() {
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 007; exec \"$@\"", "bootstrap-child"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::fs_permissions::tests::shared_bootstrap_keeps_managed_configuration_private",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("DECAPOD_SHARED_BOOTSTRAP_CHILD", "1")
            .env(SHARED_STORAGE_ENV, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let data = root.path().join(".decapod/data");
    ensure_storage_dir(&data).unwrap();
    assert_eq!(mode(&data), 0o770);
    assert_eq!(mode(data.parent().unwrap()), 0o700);
    let dockerfile =
        crate::plugins::container::prepare_generated_container_profile(root.path()).unwrap();
    assert_eq!(mode(dockerfile.parent().unwrap()), 0o700);
    assert_eq!(mode(&dockerfile), 0o600);
    crate::core::migration::check_and_migrate_with_backup(
        &root.path().join(".decapod"),
        |_| Ok(()),
    )
    .unwrap();
    let db = crate::core::db::db_connect(data.join("decapod.db").to_str().unwrap()).unwrap();
    db.execute_batch("CREATE TABLE sharing_remains_available (id INTEGER)")
        .unwrap();
}
