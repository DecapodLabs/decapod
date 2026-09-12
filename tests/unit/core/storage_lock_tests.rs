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
