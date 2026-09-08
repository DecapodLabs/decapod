//! Cross-process coordination for a local Decapod datastore.
//!
//! Dactyl remains the physical SQLite boundary. This small, Decapod-owned
//! sidecar lock coordinates Decapod processes that may otherwise reach the
//! same local file from a host and a container at the same time. The lock is
//! advisory, bounded, and held by RAII; the operating system releases it if a
//! process exits, so Decapod never guesses whether a lock file is stale and
//! never deletes one as a repair operation.

use crate::core::error::DecapodError;
use fs2::FileExt;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// The lock mode used for a local datastore operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageLockMode {
    /// Multiple read-only Decapod operations may share the lock.
    Shared,
    /// A write or schema-changing operation owns the lock exclusively.
    Exclusive,
}

/// A held local datastore coordination lock.
#[derive(Debug)]
pub struct StorageLock {
    state: Arc<HeldStorageLock>,
}

/// One OS lock shared by all canonical connections in this process for the
/// same datastore. The registry is process-local; the descriptor-backed lock
/// remains the cross-process boundary.
#[derive(Debug)]
struct HeldStorageLock {
    file: File,
    path: PathBuf,
}

impl Drop for HeldStorageLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn process_locks() -> &'static Mutex<HashMap<PathBuf, Weak<HeldStorageLock>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<HeldStorageLock>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl StorageLock {
    /// Acquire the sidecar lock within a bounded timeout.
    pub fn acquire(
        db_path: &Path,
        mode: StorageLockMode,
        timeout: Duration,
    ) -> Result<Self, DecapodError> {
        let path = lock_path(db_path);
        let mut registry = process_locks().lock().map_err(|_| {
            DecapodError::ValidationError("local storage lock registry poisoned".to_string())
        })?;
        if let Some(existing) = registry.get(&path).and_then(Weak::upgrade) {
            return Ok(Self { state: existing });
        }

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| lock_io_error(&path, error))?;

        let started = Instant::now();
        loop {
            // Use an exclusive OS lock for both logical modes. This is a
            // deliberate conservative choice for host-mounted filesystems:
            // it prevents a reader holding SQLite sidecars open while a
            // writer in another process changes the database. Same-process
            // readers still share the already-held descriptor through the
            // registry above.
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => {
                    let state = Arc::new(HeldStorageLock {
                        file,
                        path: path.clone(),
                    });
                    registry.insert(path, Arc::downgrade(&state));
                    return Ok(Self { state });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= timeout {
                        return Err(DecapodError::ValidationError(format!(
                            "STORAGE_LOCK_TIMEOUT: could not acquire the {} local datastore coordination lock for '{}' within {}ms. Another Decapod process may be using this workspace datastore; retry after it exits. Decapod does not delete lock files as stale-lock repair because the operating system releases the held lock when its owner exits.",
                            mode_name(mode),
                            path.display(),
                            timeout.as_millis()
                        )));
                    }

                    let remaining = timeout.saturating_sub(started.elapsed());
                    thread::sleep(Duration::from_millis(25).min(remaining));
                }
                Err(error) => return Err(lock_io_error(&path, error)),
            }
        }
    }

    /// Return the sidecar path for diagnostics and tests.
    pub fn path(&self) -> &Path {
        &self.state.path
    }
}

/// The lock lives beside the database and is deliberately not the database
/// file itself. Dactyl remains free to manage SQLite's journal and WAL files.
pub fn lock_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", db_path.to_string_lossy()))
}

fn mode_name(mode: StorageLockMode) -> &'static str {
    match mode {
        StorageLockMode::Shared => "shared",
        StorageLockMode::Exclusive => "exclusive",
    }
}

fn lock_io_error(path: &Path, error: std::io::Error) -> DecapodError {
    DecapodError::ValidationError(format!(
        "STORAGE_LOCK_FAILED: could not open or lock the local datastore coordination file '{}': {}. Check the parent directory permissions and local filesystem support.",
        path.display(),
        error
    ))
}

#[cfg(test)]
mod tests {
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
            .open(&sidecar)
            .expect("external lock descriptor");
        FileExt::try_lock_exclusive(&external).expect("external descriptor lock");

        let error = StorageLock::acquire(&db_path, StorageLockMode::Exclusive, Duration::ZERO)
            .expect_err("external descriptor must block the sidecar");
        assert!(error.to_string().contains("STORAGE_LOCK_TIMEOUT"));
        assert!(sidecar.exists(), "coordination files are never deleted");

        FileExt::unlock(&external).expect("release external descriptor");
    }
}
