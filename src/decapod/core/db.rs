//! Database connection and initialization utilities.
//!
//! This module provides low-level database connection primitives and
//! subsystem-specific initialization functions.

use crate::core::broker::DbBroker;
pub use crate::core::dactyl_db::{
    Connection, Error, FromSql, IntoParams, MappedRows, OpenFlags, OptionalExtension, Params,
    Result, Row, Rows, Statement, ToSql, Transaction, Type, params_from_iter, params_from_values,
    types,
};
use crate::core::error;
use crate::core::schemas; // Import the new schemas module
pub use crate::params;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STORAGE_CONNECT_MAX_RETRIES: u32 = 5;
const STORAGE_CONNECT_BASE_DELAY_MS: u64 = 50;
const STORAGE_CONNECT_MAX_DELAY_MS: u64 = 1_000;
const STORAGE_CONNECT_JITTER_MS: u64 = 37;

const UNSUPPORTED_FS_TYPES: &[&str] = &["nfs", "nfs4", "cifs", "smbfs", "9p", "vboxsf"];

/// Establish the canonical local connection through Dactyl.
///
/// The path, access policy, and retry policy remain Decapod-owned. Physical
/// opening, locking, and execution are delegated to Dactyl.
pub fn db_connect(db_path: &str) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    ensure_db_parent_dir(db_path)?;
    storage_preflight_for_db(db_path, true)?;
    open_with_retry(
        db_path,
        || {
            Connection::open_with_options(
                db_path,
                dactyl_db::AccessMode::ReadWrite,
                connection_lock_timeout(Duration::from_secs(5)),
            )
        },
        "open",
    )
}

/// Establish a read-only connection for validation probes.
pub fn db_connect_for_validate(db_path: &str) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    storage_preflight_for_db(db_path, false)?;
    let conn = open_with_retry(
        db_path,
        || {
            Connection::open_with_options(
                db_path,
                dactyl_db::AccessMode::ReadOnly,
                connection_lock_timeout(Duration::from_secs(2)),
            )
        },
        "open_readonly_validate",
    )?;
    Ok(conn)
}

/// Establish a read-write connection with a caller-specified Dactyl lock timeout.
pub fn db_connect_pooled(
    db_path: &str,
    busy_timeout_secs: u32,
) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    ensure_db_parent_dir(db_path)?;
    storage_preflight_for_db(db_path, true)?;

    open_with_retry(
        db_path,
        || {
            Connection::open_with_options(
                db_path,
                dactyl_db::AccessMode::ReadWrite,
                connection_lock_timeout(Duration::from_secs(busy_timeout_secs as u64)),
            )
        },
        "open",
    )
}

/// Establish a read-only connection with a caller-specified Dactyl lock timeout.
pub fn db_connect_read_pooled(
    db_path: &str,
    busy_timeout_secs: u32,
) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    storage_preflight_for_db(db_path, false)?;
    open_with_retry(
        db_path,
        || {
            Connection::open_with_options(
                db_path,
                dactyl_db::AccessMode::ReadOnly,
                connection_lock_timeout(Duration::from_secs(busy_timeout_secs as u64)),
            )
        },
        "open_readonly_pooled",
    )
}

/// Validation runs in a killable child process. Keep its SQLite busy wait
/// below the parent deadline so a lock contention is reported as a typed
/// storage failure instead of looking like an opaque validation timeout.
fn connection_lock_timeout(default: Duration) -> Duration {
    if std::env::var_os("DECAPOD_VALIDATE_WORKER").is_some() {
        Duration::from_millis(250)
    } else {
        default
    }
}

fn ensure_db_parent_dir(db_path: &Path) -> Result<(), error::DecapodError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn open_with_retry<F>(
    db_path: &Path,
    mut open_fn: F,
    stage: &str,
) -> Result<Connection, error::DecapodError>
where
    F: FnMut() -> Result<Connection>,
{
    let mut attempt = 0u32;
    loop {
        if let Some(injected) = injected_fault(stage, db_path) {
            return Err(injected);
        }
        match open_fn() {
            Ok(conn) => return Ok(conn),
            Err(err) => {
                if is_retryable_storage_open_error(&err) && attempt < STORAGE_CONNECT_MAX_RETRIES {
                    let delay_ms = ((STORAGE_CONNECT_BASE_DELAY_MS * 2u64.pow(attempt))
                        .min(STORAGE_CONNECT_MAX_DELAY_MS))
                        + retry_jitter_ms(attempt);
                    attempt += 1;
                    thread::sleep(Duration::from_millis(delay_ms));
                    continue;
                }
                return Err(db_open_error_with_diagnostics(db_path, stage, &err));
            }
        }
    }
}

fn is_retryable_storage_open_error(err: &crate::core::db::Error) -> bool {
    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("locked")
        || lower.contains("busy")
        || lower.contains("contention")
        || lower.contains("disk i/o")
}

fn db_open_error_with_diagnostics(
    db_path: &Path,
    stage: &str,
    err: &crate::core::db::Error,
) -> error::DecapodError {
    error::DecapodError::ValidationError(format_db_open_diagnostics(db_path, stage, err))
}

fn format_db_open_diagnostics(db_path: &Path, stage: &str, err: &crate::core::db::Error) -> String {
    let resolved = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf())
        .display()
        .to_string();
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let parent_exists = parent.exists();
    let parent_writable = if parent_exists {
        !parent
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    } else {
        false
    };

    let db_exists = db_path.exists();
    let db_writable = if db_exists {
        !db_path
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    } else {
        false
    };

    let tmp_env = std::env::var("TMPDIR").unwrap_or_else(|_| "<unset>".to_string());
    let tmp_resolved = std::env::temp_dir();
    let tmp_writable = !tmp_resolved
        .metadata()
        .map(|m| m.permissions().readonly())
        .unwrap_or(true);

    let mut hints = Vec::new();
    if !parent_exists {
        hints.push(format!(
            "create parent directory: mkdir -p {}",
            parent.display()
        ));
    }
    if parent_exists && !parent_writable {
        hints.push(format!(
            "parent directory is not writable: {}",
            parent.display()
        ));
    }
    if db_exists && !db_writable {
        hints.push(format!("database file is read-only: {}", db_path.display()));
    }
    if !tmp_writable {
        hints.push(format!(
            "TMPDIR is not writable (TMPDIR={} resolved={}): set TMPDIR to a writable directory like /tmp",
            tmp_env,
            tmp_resolved.display()
        ));
    }
    if hints.is_empty() {
        hints.push("check filesystem mount options, free space, and path permissions".to_string());
    }

    format!(
        "storage open failed at stage='{}' path='{}' parent='{}' parent_exists={} parent_writable={} db_exists={} db_writable={} TMPDIR={} temp_dir={} temp_dir_writable={} storage_error={}; remediation: {}",
        stage,
        resolved,
        parent.display(),
        parent_exists,
        parent_writable,
        db_exists,
        db_writable,
        tmp_env,
        tmp_resolved.display(),
        tmp_writable,
        err,
        hints.join("; ")
    )
}

fn retry_jitter_ms(attempt: u32) -> u64 {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    (now_ms + (attempt as u64 * 13)) % STORAGE_CONNECT_JITTER_MS
}

fn injected_fault(stage: &str, db_path: &Path) -> Option<error::DecapodError> {
    let injected = std::env::var("DECAPOD_STORAGE_FAULT_STAGE").ok()?;
    if injected != "*" && injected != stage {
        return None;
    }
    Some(error::DecapodError::ValidationError(format!(
        "STORAGE_FAULT_INJECTED: stage='{}' path='{}'",
        stage,
        db_path.display()
    )))
}

pub fn storage_health_preflight(store_root: &Path) -> Result<(), error::DecapodError> {
    if let Some(injected) = injected_fault("storage_preflight", store_root) {
        return Err(injected);
    }
    if !store_root.exists() {
        fs::create_dir_all(store_root).map_err(error::DecapodError::IoError)?;
    }
    if !store_root.is_dir() {
        return Err(error::DecapodError::ValidationError(format!(
            "STORAGE_PREFLIGHT_FAILED: store root '{}' is not a directory.",
            store_root.display()
        )));
    }
    if let Some(fs_type) = detect_fs_type(store_root) {
        let fs_type_l = fs_type.to_ascii_lowercase();
        if UNSUPPORTED_FS_TYPES.iter().any(|t| *t == fs_type_l) {
            return Err(error::DecapodError::ValidationError(format!(
                "STORAGE_PREFLIGHT_UNSUPPORTED_FS: path='{}' fs_type='{}' is not supported for Decapod local state. Use a local filesystem (ext4/xfs/apfs) and re-run.",
                store_root.display(),
                fs_type
            )));
        }
    }
    write_probe(store_root)?;
    Ok(())
}

fn storage_preflight_for_db(
    db_path: &Path,
    require_write: bool,
) -> Result<(), error::DecapodError> {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    if require_write {
        storage_health_preflight(parent)?;
    } else {
        if let Some(fs_type) = detect_fs_type(parent) {
            let fs_type_l = fs_type.to_ascii_lowercase();
            if UNSUPPORTED_FS_TYPES.iter().any(|t| *t == fs_type_l) {
                return Err(error::DecapodError::ValidationError(format!(
                    "STORAGE_PREFLIGHT_UNSUPPORTED_FS: path='{}' fs_type='{}' is not supported for Decapod local state. Use a local filesystem and retry.",
                    parent.display(),
                    fs_type
                )));
            }
        }
        if !parent.exists() {
            return Err(error::DecapodError::ValidationError(format!(
                "STORAGE_PREFLIGHT_FAILED: parent directory '{}' does not exist.",
                parent.display()
            )));
        }
    }
    Ok(())
}

fn write_probe(dir: &Path) -> Result<(), error::DecapodError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let probe = dir.join(format!(
        ".decapod-write-probe-{}-{}.tmp",
        std::process::id(),
        nanos
    ));
    fs::write(&probe, b"ok").map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "STORAGE_PREFLIGHT_FAILED: write probe failed at '{}': {}. Check directory permissions, mount mode, and available disk/inodes.",
            probe.display(),
            e
        ))
    })?;
    fs::remove_file(&probe).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "STORAGE_PREFLIGHT_FAILED: cleanup probe failed at '{}': {}. Check filesystem health.",
            probe.display(),
            e
        ))
    })?;
    Ok(())
}

fn detect_fs_type(path: &Path) -> Option<String> {
    let canon = path.canonicalize().ok()?;
    let text = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let mut best: Option<(usize, String)> = None;
    for line in text.lines() {
        let mut parts = line.split(" - ");
        let Some(left) = parts.next() else {
            continue;
        };
        let Some(right) = parts.next() else {
            continue;
        };
        let left_parts: Vec<&str> = left.split_whitespace().collect();
        if left_parts.len() < 5 {
            continue;
        }
        let mount_point = left_parts[4].replace("\\040", " ");
        let mount_point_path = Path::new(&mount_point);
        if !canon.starts_with(mount_point_path) {
            continue;
        }
        let right_parts: Vec<&str> = right.split_whitespace().collect();
        if right_parts.is_empty() {
            continue;
        }
        let fs_type = right_parts[0].to_string();
        let mlen = mount_point_path.as_os_str().len();
        match &best {
            Some((best_len, _)) if *best_len >= mlen => {}
            _ => best = Some((mlen, fs_type)),
        }
    }
    best.map(|(_, fs)| fs)
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LOCAL_DB_NAME)
}

pub fn initialize_knowledge_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = knowledge_db_path(root);
    let parent_dir = db_path.parent().unwrap();
    fs::create_dir_all(parent_dir).map_err(error::DecapodError::IoError)?;

    // The canonical datastore also owns the todo policy tables used by the
    // broker before it opens any subsystem connection.
    crate::core::todo::initialize_todo_db(root)?;

    let broker = DbBroker::new(root);
    broker.with_conn(&db_path, "decapod", None, "knowledge.init", |conn| {
        conn.execute(schemas::KNOWLEDGE_DB_SCHEMA, [])?;
        ensure_knowledge_columns(conn)?;
        conn.execute(schemas::KNOWLEDGE_DB_INDEX_STATUS, [])?;
        conn.execute(schemas::KNOWLEDGE_DB_INDEX_CREATED, [])?;
        conn.execute(schemas::KNOWLEDGE_DB_INDEX_MERGE_KEY, [])?;
        conn.execute(schemas::KNOWLEDGE_DB_INDEX_ACTIVE_MERGE_SCOPE, [])?;
        Ok(())
    })?;

    Ok(())
}

/// Migrate existing knowledge tables to add new columns if missing.
fn ensure_knowledge_columns(
    conn: &crate::core::db::Connection,
) -> Result<(), crate::core::db::Error> {
    let cols = conn
        .columns("knowledge")?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();

    let add_col =
        |name: &str, sql_type: &str, default_expr: &str| -> Result<(), crate::core::db::Error> {
            if !cols.contains(name) {
                conn.execute(
                    &format!(
                        "ALTER TABLE knowledge ADD COLUMN {name} {sql_type} DEFAULT {default_expr}"
                    ),
                    [],
                )?;
            }
            Ok(())
        };

    add_col("status", "TEXT NOT NULL", "'active'")?;
    add_col("merge_key", "TEXT", "''")?;
    add_col("supersedes_id", "TEXT", "NULL")?;
    add_col("ttl_policy", "TEXT NOT NULL", "'persistent'")?;
    add_col("expires_ts", "TEXT", "NULL")?;
    Ok(())
}

pub fn decide_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LOCAL_DB_NAME)
}

pub fn initialize_decide_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = decide_db_path(root);
    let parent_dir = db_path.parent().unwrap();
    fs::create_dir_all(parent_dir).map_err(error::DecapodError::IoError)?;

    let broker = DbBroker::new(root);
    broker.with_conn(&db_path, "decapod", None, "decide.init", |conn| {
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_META)?;
        conn.execute_batch(schemas::DECIDE_DB_SCHEMA_SESSIONS)?;
        conn.execute_batch(schemas::DECIDE_DB_SCHEMA_DECISIONS)?;
        conn.execute_batch(schemas::DECIDE_DB_INDEX_DECISIONS_SESSION)?;
        conn.execute_batch(schemas::DECIDE_DB_INDEX_DECISIONS_TREE)?;
        conn.execute_batch(schemas::DECIDE_DB_INDEX_SESSIONS_TREE)?;
        conn.execute_batch(schemas::DECIDE_DB_INDEX_SESSIONS_STATUS)?;
        Ok(())
    })?;

    Ok(())
}

// Subsystems own their schemas and initialization. Avoid generic "plugin DB" APIs until
// a real extension mechanism exists.
