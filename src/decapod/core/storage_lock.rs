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
    /// Physical connections share file-lifetime protection; SQLite locks transactions.
    Shared,
    /// Maintenance or file replacement excludes every physical connection.
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
    mode: StorageLockMode,
    thread: std::thread::ThreadId,
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
        // Canonicalize the parent even before the database exists, so aliases
        // and creation/replacement retain the same stable lock inode.
        let parent = db_path.parent().unwrap_or(Path::new(".")).canonicalize()?;
        let path = lock_path(&parent.join(db_path.file_name().ok_or_else(|| {
            DecapodError::ValidationError("STORAGE_LOCK_DATABASE_NAME_MISSING".into())
        })?));
        crate::core::fs_permissions::check_storage(db_path)
            .map_err(|error| lock_io_error(&path, error))?;
        let mut registry = process_locks().lock().map_err(|_| {
            DecapodError::ValidationError("local storage lock registry poisoned".to_string())
        })?;
        if let Some(existing) = registry.get(&path).and_then(Weak::upgrade) {
            if mode == StorageLockMode::Exclusive && existing.mode == StorageLockMode::Shared {
                return Err(DecapodError::ValidationError(
                    "STORAGE_LOCK_UPGRADE_BLOCKED: close physical connections before maintenance"
                        .into(),
                ));
            }
            if existing.mode == StorageLockMode::Shared
                || existing.thread == std::thread::current().id()
            {
                return Ok(Self { state: existing });
            }
        }

        let file = crate::core::fs_permissions::open_storage_file(
            &path,
            OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false),
        )
        .map_err(|error| lock_io_error(&path, error))?;

        let started = Instant::now();
        loop {
            // Shared locks protect file lifetime, not SQL transactions. Only
            // migration/restore/maintenance owns exclusive lifecycle access.
            let acquired = match mode {
                StorageLockMode::Shared => FileExt::try_lock_shared(&file),
                StorageLockMode::Exclusive => FileExt::try_lock_exclusive(&file),
            };
            match acquired {
                Ok(()) => {
                    let state = Arc::new(HeldStorageLock {
                        file,
                        path: path.clone(),
                        mode,
                        thread: std::thread::current().id(),
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
                    // Contention on one datastore must not hold the registry
                    // mutex across waits for unrelated datastores.
                    drop(registry);
                    thread::sleep(Duration::from_millis(25).min(remaining));
                    registry = process_locks().lock().map_err(|_| {
                        DecapodError::ValidationError("local storage lock registry poisoned".into())
                    })?;
                    if let Some(existing) = registry.get(&path).and_then(Weak::upgrade) {
                        if mode == StorageLockMode::Exclusive
                            && existing.mode == StorageLockMode::Shared
                        {
                            return Err(DecapodError::ValidationError("STORAGE_LOCK_UPGRADE_BLOCKED: close physical connections before maintenance".into()));
                        }
                        if existing.mode == StorageLockMode::Shared
                            || existing.thread == std::thread::current().id()
                        {
                            return Ok(Self { state: existing });
                        }
                    }
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
#[path = "../../../tests/unit/core/storage_lock_tests.rs"]
mod tests;
