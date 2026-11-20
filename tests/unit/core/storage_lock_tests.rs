// Moved from src/decapod/core/storage_lock.rs
use super::*;
use tempfile::tempdir;

#[test]
fn lock_path_is_a_sidecar_of_the_database() {
    let path = Path::new(".decapod/data/decapod.db");
    assert_eq!(
        lock_path(path),
        PathBuf::from(".decapod/data/decapod.db.lock")
    );
}

#[test]
fn same_process_connections_share_the_held_lock() {
    let directory = tempdir().expect("temporary directory");
    let db_path = directory.path().join("decapod.db");
    let held = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO)
        .expect("first lock");

    // A second canonical connection in this process joins the held OS
    // lock instead of deadlocking on the process's own descriptor.
    let nested = StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO)
        .expect("nested lock joins the process-local registry");
    drop(held);
    drop(nested);

    // The sidecar is reusable once the final canonical connection drops.
    let released = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO)
        .expect("lock after final release");
    drop(released);
}

#[test]
fn external_descriptor_contention_is_reported_without_stale_cleanup() {
    let directory = tempdir().expect("temporary directory");
    let db_path = directory.path().join("decapod.db");
    let sidecar = lock_path(&db_path);
    let external = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&sidecar)
        .expect("external lock descriptor");
    FileExt::try_lock_exclusive(&external).expect("external descriptor lock");

    let error = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO)
        .expect_err("external descriptor must block the sidecar");
    assert!(error.to_string().contains("STORAGE_LOCK_TIMEOUT"));
    assert!(sidecar.exists(), "coordination files are never deleted");

    FileExt::unlock(&external).expect("release external descriptor");
}

#[test]
fn shared_lifecycle_guards_allow_other_handles_but_exclude_replacement() {
    let directory = tempdir().unwrap();
    let db_path = directory.path().join("decapod.db");
    let held = StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO).unwrap();
    let external = OpenOptions::new()
        .read(true)
        .write(true)
        .open(held.path())
        .unwrap();
    FileExt::try_lock_shared(&external).unwrap();
    assert!(
        StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO)
            .unwrap_err()
            .to_string()
            .contains("STORAGE_LOCK_UPGRADE_BLOCKED")
    );
    FileExt::unlock(&external).unwrap();
    assert!(FileExt::try_lock_exclusive(&external).is_err());
    drop(held);
    FileExt::try_lock_exclusive(&external).unwrap();
    assert!(lock_path(&db_path).exists());
}

#[test]
fn maintenance_guard_is_not_inherited_by_an_unrelated_thread() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("decapod.db");
    let guard = StorageLock::acquire(&path, StorageLockMode::Exclusive, Duration::ZERO).unwrap();
    let error = std::thread::spawn(move || {
        StorageLock::acquire(&path, StorageLockMode::Shared, Duration::ZERO).unwrap_err()
    })
    .join()
    .unwrap();
    assert!(error.to_string().contains("STORAGE_LOCK_TIMEOUT"));
    drop(guard);
}

#[test]
fn diagnose_missing_or_unlocked_store_never_creates_or_deletes_sidecars() {
    let directory = tempdir().unwrap();
    let db_path = directory.path().join("decapod.db");
    let sidecar = lock_path(&db_path);

    let missing = StorageLock::diagnose(&db_path, Duration::ZERO).unwrap();
    assert_eq!(missing.lock_state, StorageLockContentionState::Unlocked);
    assert_eq!(missing.owner, StorageLockOwnerKind::None);
    assert!(!missing.database_exists);
    assert!(!missing.lock_file_exists);
    assert!(!sidecar.exists(), "diagnose must not create a sidecar lock");

    std::fs::write(&db_path, b"").unwrap();
    let wal = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
    let shm = PathBuf::from(format!("{}-shm", db_path.to_string_lossy()));
    std::fs::write(&wal, b"wal").unwrap();
    std::fs::write(&shm, b"shm").unwrap();
    let held = StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO).unwrap();
    drop(held);

    let unlocked = StorageLock::diagnose(&db_path, Duration::ZERO).unwrap();
    assert_eq!(unlocked.lock_state, StorageLockContentionState::Unlocked);
    assert_eq!(unlocked.owner, StorageLockOwnerKind::None);
    assert!(unlocked.database_exists);
    assert!(unlocked.lock_file_exists);
    assert!(unlocked.wal_exists);
    assert!(unlocked.shm_exists);
    assert!(!unlocked.timed_out);
    assert!(sidecar.exists(), "diagnose must preserve sidecar lock file");
    assert!(wal.exists(), "diagnose must preserve -wal sidecar");
    assert!(shm.exists(), "diagnose must preserve -shm sidecar");
}

#[test]
fn diagnose_distinguishes_current_process_and_external_shared_or_exclusive_contention() {
    let directory = tempdir().unwrap();
    let db_path = directory.path().join("decapod.db");
    std::fs::write(&db_path, b"").unwrap();
    let sidecar = lock_path(&db_path);

    let shared_guard =
        StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO).unwrap();
    let in_proc_shared = StorageLock::diagnose(&db_path, Duration::ZERO).unwrap();
    assert_eq!(
        in_proc_shared.lock_state,
        StorageLockContentionState::SharedConnectionsOpen
    );
    assert_eq!(in_proc_shared.owner, StorageLockOwnerKind::CurrentProcess);
    assert_eq!(
        in_proc_shared.in_process_mode,
        Some(StorageLockMode::Shared)
    );
    assert_eq!(in_proc_shared.in_process_refs, 1);
    drop(shared_guard);

    let exclusive_guard =
        StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO).unwrap();
    let in_proc_excl = StorageLock::diagnose(&db_path, Duration::ZERO).unwrap();
    assert_eq!(
        in_proc_excl.lock_state,
        StorageLockContentionState::ExclusiveMaintenanceActive
    );
    assert_eq!(in_proc_excl.owner, StorageLockOwnerKind::CurrentProcess);
    assert_eq!(
        in_proc_excl.in_process_mode,
        Some(StorageLockMode::Exclusive)
    );
    drop(exclusive_guard);

    // Simulate external shared connection descriptor not registered in process_locks().
    let external = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&sidecar)
        .unwrap();
    FileExt::try_lock_shared(&external).unwrap();
    let ext_shared = StorageLock::diagnose(&db_path, Duration::from_millis(25)).unwrap();
    assert_eq!(
        ext_shared.lock_state,
        StorageLockContentionState::SharedConnectionsOpen
    );
    assert_eq!(ext_shared.owner, StorageLockOwnerKind::Unknown);
    assert!(ext_shared.timed_out);
    assert!(sidecar.exists());
    FileExt::unlock(&external).unwrap();

    // Simulate external exclusive maintenance descriptor.
    FileExt::try_lock_exclusive(&external).unwrap();
    let ext_excl = StorageLock::diagnose(&db_path, Duration::from_millis(25)).unwrap();
    assert_eq!(
        ext_excl.lock_state,
        StorageLockContentionState::ExclusiveMaintenanceActive
    );
    assert_eq!(ext_excl.owner, StorageLockOwnerKind::Unknown);
    assert!(ext_excl.timed_out);
    assert!(sidecar.exists());
    FileExt::unlock(&external).unwrap();
}

#[test]
fn release_owned_if_idle_relinquishes_registered_lease_and_refuses_active_or_foreign_locks() {
    let directory = tempdir().unwrap();
    let db_path = directory.path().join("decapod.db");
    std::fs::write(&db_path, b"").unwrap();
    let sidecar = lock_path(&db_path);

    let noop = StorageLock::release_owned_if_idle(&db_path).unwrap();
    assert_eq!(noop.status, "noop_unlocked");
    assert!(!noop.released);

    // Live unregistered handle cannot be forcibly dropped.
    let active = StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO).unwrap();
    let active_err = StorageLock::release_owned_if_idle(&db_path).unwrap_err();
    assert!(
        active_err
            .to_string()
            .contains("LOCK_LEASE_ACTIVE_UNREGISTERED")
    );
    drop(active);

    // Explicitly registered idle lease is relinquished cleanly without deleting sidecar.
    let lease = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO).unwrap();
    lease.register_releasable_own_lease().unwrap();
    let released = StorageLock::release_owned_if_idle(&db_path).unwrap();
    assert_eq!(released.status, "released");
    assert!(released.released);
    assert_eq!(released.previous_mode, Some(StorageLockMode::Exclusive));
    assert!(released.lock_file_preserved);
    assert!(sidecar.exists());

    // Foreign external descriptor lock is refused without unlocking or unlinking.
    let external = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&sidecar)
        .unwrap();
    FileExt::try_lock_exclusive(&external).unwrap();
    let foreign_err = StorageLock::release_owned_if_idle(&db_path).unwrap_err();
    assert!(foreign_err.to_string().contains("FOREIGN_LOCK_NOT_OWNED"));
    assert!(sidecar.exists());
    assert!(
        StorageLock::acquire(&db_path, StorageLockMode::Shared, Duration::ZERO).is_err(),
        "external lock must remain intact"
    );
    FileExt::unlock(&external).unwrap();
}
