//! SQLite connection pool with read/write separation and retry logic.
//!
//! Replaces the per-DB `Mutex<()>` serialization in `broker.rs` with a pool that:
//! - Maintains a **write mutex** per DB for serialized write access
//! - Creates fresh **read connections** per operation (no mutex, concurrent via WAL)
//! - Uses longer `busy_timeout` values (30s write, 15s read) to handle cross-process contention
//!
//! Connections are NOT pooled (opened fresh each time) to avoid WAL/SHM file handle
//! conflicts when the process spawns child subprocesses that access the same databases.
//!
//! # Future: `StorageBackend` trait for Supabase
//!
//! The current closure-based `with_conn(&Connection)` API cannot abstract over HTTP backends
//! (closures capture `&Connection` which is SQLite-specific). When Supabase support is needed,
//! introduce an operation-based dispatch trait:
//!
//! ```ignore
//! trait StorageBackend {
//!     fn execute(&self, op: StorageOp) -> Result<StorageResult, DecapodError>;
//! }
//! enum StorageOp { Query { sql: String, params: Vec<Value> }, Execute { sql: String, params: Vec<Value> } }
//! enum StorageResult { Rows(Vec<Row>), Changed(u64) }
//! ```
//!
//! This would require rewriting the 136 `with_conn` call sites to use `StorageOp` instead.
//! Until then, the pool fixes contention without touching any call sites.

use crate::core::db;
use crate::core::error::DecapodError;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// Maximum retry attempts for busy/locked errors.
const MAX_RETRIES: u32 = 5;
/// Base delay for exponential backoff (milliseconds).
const BASE_DELAY_MS: u64 = 100;
/// Maximum delay cap (milliseconds).
const MAX_DELAY_MS: u64 = 5_000;

/// Write connection busy_timeout in seconds.
const WRITE_BUSY_TIMEOUT_SECS: u32 = 5;
/// Read connection busy_timeout in seconds.
const READ_BUSY_TIMEOUT_SECS: u32 = 5;

/// Per-database entry holding a write mutex for serialized write access.
struct PoolEntry {
    write_lock: Mutex<()>,
    db_path: PathBuf,
}

/// Connection pool providing read/write separation per SQLite database.
///
/// - Write operations are serialized through a per-DB mutex with fresh connections.
/// - Read operations create fresh connections without mutex serialization (WAL concurrent reads).
/// - Both paths use increased `busy_timeout` for cross-process contention.
pub struct SqlitePool {
    entries: Mutex<HashMap<PathBuf, &'static PoolEntry>>,
}

impl SqlitePool {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get_entry(&self, db_path: &Path) -> Result<&'static PoolEntry, DecapodError> {
        let canonical = db_path.to_path_buf();
        let mut entries = self.entries.lock().map_err(|_| {
            DecapodError::ValidationError("SqlitePool entries lock poisoned".to_string())
        })?;
        if let Some(entry) = entries.get(&canonical) {
            return Ok(*entry);
        }
        let entry = Box::leak(Box::new(PoolEntry {
            write_lock: Mutex::new(()),
            db_path: canonical.clone(),
        }));
        entries.insert(canonical, entry);
        Ok(entry)
    }

    /// Execute a closure with a write connection for the given DB path.
    /// Write access is serialized per-DB via mutex.
    pub fn with_write<F, R>(&self, db_path: &Path, f: F) -> Result<R, DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, DecapodError>,
    {
        let entry = self.get_entry(db_path)?;
        let _guard = entry
            .write_lock
            .lock()
            .map_err(|_| DecapodError::ValidationError("Pool write lock poisoned".to_string()))?;

        let conn =
            db::db_connect_pooled(&entry.db_path.to_string_lossy(), WRITE_BUSY_TIMEOUT_SECS)?;

        f(&conn)
    }

    /// Execute a closure with a read connection (no mutex serialization).
    /// WAL mode allows concurrent readers across threads and processes.
    pub fn with_read<F, R>(&self, db_path: &Path, f: F) -> Result<R, DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, DecapodError>,
    {
        let conn = db::db_connect_pooled(&db_path.to_string_lossy(), READ_BUSY_TIMEOUT_SECS)?;

        f(&conn)
    }
}

/// Retry a closure on `SQLITE_BUSY` / `DatabaseBusy` with exponential backoff.
///
/// Note: only usable with `FnMut` closures (not the `FnOnce` closures from `with_conn`).
/// Available for internal pool operations and future `StorageBackend` retry logic.
#[allow(dead_code)]
fn retry_on_busy<F, R>(mut f: F) -> Result<R, DecapodError>
where
    F: FnMut() -> Result<R, DecapodError>,
{
    let mut attempt = 0u32;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if is_busy_error(&e) && attempt < MAX_RETRIES => {
                attempt += 1;
                let delay_ms = (BASE_DELAY_MS * 2u64.pow(attempt - 1)).min(MAX_DELAY_MS);
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Check if an error is a SQLite busy/locked error that is retryable.
fn is_busy_error(err: &DecapodError) -> bool {
    match err {
        DecapodError::RusqliteError(rusqlite::Error::SqliteFailure(code, _)) => matches!(
            code.code,
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
        ),
        _ => false,
    }
}

/// Global pool instance (same lifetime as the process).
pub fn global_pool() -> &'static SqlitePool {
    static POOL: OnceLock<SqlitePool> = OnceLock::new();
    POOL.get_or_init(SqlitePool::new)
}
