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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum probe window permitted for non-destructive lock diagnosis.
pub const MAX_DIAGNOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// The lock mode used for a local datastore operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageLockMode {
    /// Physical connections share file-lifetime protection; SQLite locks transactions.
    Shared,
    /// Maintenance or file replacement excludes every physical connection.
    Exclusive,
}

/// Distinguishable advisory lock state observed without mutating sidecar files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageLockContentionState {
    Unlocked,
    SharedConnectionsOpen,
    ExclusiveMaintenanceActive,
}

/// Knowable lock ownership within Decapod's process boundary.
///
/// Decapod never guesses or claims global OS PID ownership from advisory sidecar
/// files; cross-process holders are reported honestly as `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageLockOwnerKind {
    None,
    CurrentProcess,
    Unknown,
}

/// Structured, non-destructive lock and sidecar health diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageLockDiagnosis {
    pub lock_state: StorageLockContentionState,
    pub owner: StorageLockOwnerKind,
    pub in_process_mode: Option<StorageLockMode>,
    pub in_process_refs: usize,
    pub foreground_owner_active: bool,
    pub database_exists: bool,
    pub lock_file_exists: bool,
    pub wal_exists: bool,
    pub shm_exists: bool,
    pub timed_out: bool,
    pub probe_timeout_ms: u64,
    pub probe_elapsed_ms: u64,
    pub recommended_next_action: &'static str,
}

/// Outcome of a safe `db.lock.release_own` attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseOwnLockReport {
    pub status: &'static str,
    pub released: bool,
    pub previous_mode: Option<StorageLockMode>,
    pub owner: StorageLockOwnerKind,
    pub lock_file_preserved: bool,
    pub detail: String,
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

fn releasable_own_leases() -> &'static Mutex<HashMap<PathBuf, StorageLock>> {
    static LEASES: OnceLock<Mutex<HashMap<PathBuf, StorageLock>>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
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

    /// Return the held lock mode.
    pub fn mode(&self) -> StorageLockMode {
        self.state.mode
    }

    /// Non-destructively diagnose current datastore coordination lock and sidecar
    /// state within a bounded probe timeout.
    ///
    /// Never creates or deletes `.lock`, `-wal`, or `-shm` files and never claims
    /// global OS process knowledge beyond `current_process` vs `unknown`.
    pub fn diagnose(
        db_path: &Path,
        probe_timeout: Duration,
    ) -> Result<StorageLockDiagnosis, DecapodError> {
        if probe_timeout > MAX_DIAGNOSE_TIMEOUT {
            return Err(DecapodError::ValidationError(format!(
                "STORAGE_LOCK_DIAGNOSE_TIMEOUT_BUDGET: requested {}ms exceeds maximum {}ms",
                probe_timeout.as_millis(),
                MAX_DIAGNOSE_TIMEOUT.as_millis()
            )));
        }
        let started = Instant::now();
        let (canonical_db, sidecar) = resolve_existing_or_logical_paths(db_path)?;
        let database_exists = canonical_db.is_file();
        let lock_file_exists = sidecar.is_file();
        let wal_exists = sidecar_suffix(&canonical_db, "-wal").is_file();
        let shm_exists = sidecar_suffix(&canonical_db, "-shm").is_file();
        #[cfg(unix)]
        let foreground_owner_active = if canonical_db.parent().is_some_and(|parent| parent.is_dir())
        {
            crate::core::storage_owner::owner_is_active(&canonical_db)?
        } else {
            false
        };
        #[cfg(not(unix))]
        let foreground_owner_active = false;

        let in_process = {
            let registry = process_locks().lock().map_err(|_| {
                DecapodError::ValidationError("local storage lock registry poisoned".into())
            })?;
            registry.get(&sidecar).and_then(Weak::upgrade).map(|held| {
                // Subtract the temporary Arc clone held only during this inspection.
                let refs = Arc::strong_count(&held).saturating_sub(1);
                (held.mode, refs.max(1))
            })
        };

        if let Some((mode, in_process_refs)) = in_process {
            let lock_state = match mode {
                StorageLockMode::Shared => StorageLockContentionState::SharedConnectionsOpen,
                StorageLockMode::Exclusive => {
                    StorageLockContentionState::ExclusiveMaintenanceActive
                }
            };
            return Ok(StorageLockDiagnosis {
                lock_state,
                owner: StorageLockOwnerKind::CurrentProcess,
                in_process_mode: Some(mode),
                in_process_refs,
                foreground_owner_active,
                database_exists,
                lock_file_exists,
                wal_exists,
                shm_exists,
                timed_out: false,
                probe_timeout_ms: probe_timeout.as_millis() as u64,
                probe_elapsed_ms: started.elapsed().as_millis() as u64,
                recommended_next_action: recommended_action(lock_state, foreground_owner_active),
            });
        }

        if !lock_file_exists {
            let lock_state = StorageLockContentionState::Unlocked;
            return Ok(StorageLockDiagnosis {
                lock_state,
                owner: StorageLockOwnerKind::None,
                in_process_mode: None,
                in_process_refs: 0,
                foreground_owner_active,
                database_exists,
                lock_file_exists: false,
                wal_exists,
                shm_exists,
                timed_out: false,
                probe_timeout_ms: probe_timeout.as_millis() as u64,
                probe_elapsed_ms: started.elapsed().as_millis() as u64,
                recommended_next_action: recommended_action(lock_state, foreground_owner_active),
            });
        }

        crate::core::fs_permissions::check_storage(&canonical_db)
            .map_err(|error| lock_io_error(&sidecar, error))?;
        let probe_file = crate::core::fs_permissions::open_storage_file(
            &sidecar,
            OpenOptions::new()
                .create(false)
                .read(true)
                .write(true)
                .truncate(false),
        )
        .map_err(|error| lock_io_error(&sidecar, error))?;

        loop {
            match FileExt::try_lock_exclusive(&probe_file) {
                Ok(()) => {
                    let _ = FileExt::unlock(&probe_file);
                    let lock_state = StorageLockContentionState::Unlocked;
                    return Ok(StorageLockDiagnosis {
                        lock_state,
                        owner: StorageLockOwnerKind::None,
                        in_process_mode: None,
                        in_process_refs: 0,
                        foreground_owner_active,
                        database_exists,
                        lock_file_exists: true,
                        wal_exists,
                        shm_exists,
                        timed_out: false,
                        probe_timeout_ms: probe_timeout.as_millis() as u64,
                        probe_elapsed_ms: started.elapsed().as_millis() as u64,
                        recommended_next_action: recommended_action(
                            lock_state,
                            foreground_owner_active,
                        ),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() < probe_timeout {
                        let remaining = probe_timeout.saturating_sub(started.elapsed());
                        thread::sleep(Duration::from_millis(10).min(remaining));
                        continue;
                    }
                    let lock_state = match FileExt::try_lock_shared(&probe_file) {
                        Ok(()) => {
                            let _ = FileExt::unlock(&probe_file);
                            StorageLockContentionState::SharedConnectionsOpen
                        }
                        Err(shared_err) if shared_err.kind() == std::io::ErrorKind::WouldBlock => {
                            StorageLockContentionState::ExclusiveMaintenanceActive
                        }
                        Err(shared_err) => return Err(lock_io_error(&sidecar, shared_err)),
                    };
                    return Ok(StorageLockDiagnosis {
                        lock_state,
                        owner: StorageLockOwnerKind::Unknown,
                        in_process_mode: None,
                        in_process_refs: 0,
                        foreground_owner_active,
                        database_exists,
                        lock_file_exists: true,
                        wal_exists,
                        shm_exists,
                        timed_out: !probe_timeout.is_zero(),
                        probe_timeout_ms: probe_timeout.as_millis() as u64,
                        probe_elapsed_ms: started.elapsed().as_millis() as u64,
                        recommended_next_action: recommended_action(
                            lock_state,
                            foreground_owner_active,
                        ),
                    });
                }
                Err(error) => return Err(lock_io_error(&sidecar, error)),
            }
        }
    }

    /// Register an explicitly owned, idle-releasable lease held by this process.
    pub fn register_releasable_own_lease(self) -> Result<(), DecapodError> {
        let mut leases = releasable_own_leases().lock().map_err(|_| {
            DecapodError::ValidationError("releasable storage lease registry poisoned".into())
        })?;
        leases.insert(self.state.path.clone(), self);
        Ok(())
    }

    /// Safely relinquish an explicitly owned, idle current-process lease on
    /// `db_path` without touching sidecar files, dropping active shared handles,
    /// or interfering with another process.
    pub fn release_owned_if_idle(db_path: &Path) -> Result<ReleaseOwnLockReport, DecapodError> {
        let (_canonical_db, sidecar) = resolve_existing_or_logical_paths(db_path)?;
        {
            let mut leases = releasable_own_leases().lock().map_err(|_| {
                DecapodError::ValidationError("releasable storage lease registry poisoned".into())
            })?;
            if let Some(owned) = leases.remove(&sidecar) {
                if Arc::strong_count(&owned.state) > 1 {
                    let mode = owned.state.mode;
                    leases.insert(sidecar.clone(), owned);
                    return Err(DecapodError::ValidationError(format!(
                        "LOCK_LEASE_IN_USE: current process still has active {:?} connection handles; refuse to drop live lease",
                        mode
                    )));
                }
                let previous_mode = owned.state.mode;
                drop(owned);
                return Ok(ReleaseOwnLockReport {
                    status: "released",
                    released: true,
                    previous_mode: Some(previous_mode),
                    owner: StorageLockOwnerKind::CurrentProcess,
                    lock_file_preserved: sidecar.is_file(),
                    detail:
                        "Relinquished idle current-process storage lease by closing its descriptor"
                            .into(),
                });
            }
        }

        let diagnosis = Self::diagnose(db_path, Duration::ZERO)?;
        match (diagnosis.lock_state, diagnosis.owner) {
            (StorageLockContentionState::Unlocked, _) => Ok(ReleaseOwnLockReport {
                status: "noop_unlocked",
                released: false,
                previous_mode: None,
                owner: StorageLockOwnerKind::None,
                lock_file_preserved: sidecar.is_file(),
                detail: "Datastore coordination lock is already unlocked; sidecar files left untouched".into(),
            }),
            (_, StorageLockOwnerKind::CurrentProcess) => Err(DecapodError::ValidationError(
                "LOCK_LEASE_ACTIVE_UNREGISTERED: current-process storage lock belongs to a live un-registered connection scope and cannot be forcibly dropped".into(),
            )),
            (_, StorageLockOwnerKind::Unknown | StorageLockOwnerKind::None) => {
                Err(DecapodError::ValidationError(
                    "FOREIGN_LOCK_NOT_OWNED: coordination lock is held by an external process (`unknown` owner); Decapod never forcibly unlocks external descriptors or deletes sidecar files".into(),
                ))
            }
        }
    }
}

fn resolve_existing_or_logical_paths(db_path: &Path) -> Result<(PathBuf, PathBuf), DecapodError> {
    let file_name = db_path.file_name().ok_or_else(|| {
        DecapodError::ValidationError("STORAGE_LOCK_DATABASE_NAME_MISSING".into())
    })?;
    let raw_parent = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let resolved_parent = if raw_parent.exists() {
        raw_parent.canonicalize()?
    } else {
        raw_parent.to_path_buf()
    };
    let canonical_db = resolved_parent.join(file_name);
    let sidecar = lock_path(&canonical_db);
    Ok((canonical_db, sidecar))
}

fn sidecar_suffix(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", db_path.to_string_lossy()))
}

fn recommended_action(
    state: StorageLockContentionState,
    foreground_owner_active: bool,
) -> &'static str {
    if foreground_owner_active {
        return "Wait for the active foreground storage broker session to drain before running exclusive maintenance, or use bounded `db.lock.retry`";
    }
    match state {
        StorageLockContentionState::Unlocked => {
            "No lock contention detected; authorized `db.verify` or `db.backup` may proceed"
        }
        StorageLockContentionState::SharedConnectionsOpen => {
            "Shared physical connections are open; ordinary read/write work can share file lifetime, while exclusive maintenance (`db.verify`/`db.backup`) should use bounded `db.lock.retry`"
        }
        StorageLockContentionState::ExclusiveMaintenanceActive => {
            "Exclusive maintenance or migration is active in another handle; wait or invoke bounded `db.lock.retry` and never delete `.lock`, `-wal`, or `-shm` files"
        }
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
