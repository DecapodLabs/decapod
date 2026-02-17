//! Database connection and initialization utilities.
//!
//! This module provides low-level database connection primitives and
//! subsystem-specific initialization functions.
//!
//! # For AI Agents
//!
//! - **Always use DbBroker**: Don't call `db_connect` directly; use broker for mutations
//! - **WAL mode enabled**: Write-Ahead Logging for better concurrency
//! - **Foreign keys enforced**: Referential integrity is ON by default
//! - **Busy timeout**: 5-second retry window for lock contention
//! - **Subsystems own their schemas**: Each subsystem (TODO, health, etc.) has its own init function

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas; // Import the new schemas module
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Path, PathBuf};

/// Establish a SQLite connection with Decapod's standard configuration.
///
/// Enables:
/// - WAL (Write-Ahead Logging) mode for better concurrency
/// - Foreign key constraints
/// - 5-second busy timeout for lock contention
///
/// # Agent Usage
///
/// Do NOT use this function directly for state mutations. Always go through
/// `DbBroker::with_conn` to ensure serialization and audit logging.
pub fn db_connect(db_path: &str) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    ensure_db_parent_dir(db_path)?;

    let conn = Connection::open(db_path)
        .map_err(|e| db_open_error_with_diagnostics(db_path, "open", &e))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| db_open_error_with_diagnostics(db_path, "busy_timeout", &e))?;
    conn.execute("PRAGMA foreign_keys=ON;", [])
        .map_err(|e| db_open_error_with_diagnostics(db_path, "foreign_keys", &e))?;
    configure_journal_mode_with_fallback(&conn, db_path)?;
    Ok(conn)
}

/// Establish a read-only SQLite connection for validation probes.
///
/// This connection avoids WAL transitions and TMPDIR-dependent temp files by:
/// - opening read-only
/// - forcing temp_store=MEMORY
/// - enabling query_only mode
pub fn db_connect_for_validate(db_path: &str) -> Result<Connection, error::DecapodError> {
    let db_path = Path::new(db_path);
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(db_path, flags)
        .map_err(|e| db_open_error_with_diagnostics(db_path, "open_readonly_validate", &e))?;
    conn.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| db_open_error_with_diagnostics(db_path, "busy_timeout_validate", &e))?;
    conn.execute("PRAGMA query_only=ON;", [])
        .map_err(|e| db_open_error_with_diagnostics(db_path, "query_only_validate", &e))?;
    conn.execute("PRAGMA temp_store=MEMORY;", [])
        .map_err(|e| db_open_error_with_diagnostics(db_path, "temp_store_validate", &e))?;
    conn.execute("PRAGMA foreign_keys=ON;", [])
        .map_err(|e| db_open_error_with_diagnostics(db_path, "foreign_keys_validate", &e))?;
    Ok(conn)
}

fn ensure_db_parent_dir(db_path: &Path) -> Result<(), error::DecapodError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn configure_journal_mode_with_fallback(
    conn: &Connection,
    db_path: &Path,
) -> Result<(), error::DecapodError> {
    match conn.query_row("PRAGMA journal_mode=WAL;", [], |_| Ok(())) {
        Ok(_) => Ok(()),
        Err(wal_err) => {
            // WAL can fail on read-only/overlay/network filesystems; DELETE is safer.
            conn.query_row("PRAGMA journal_mode=DELETE;", [], |_| Ok(()))
                .map_err(|delete_err| {
                    error::DecapodError::ValidationError(format!(
                        "{}; fallback journal_mode=DELETE also failed: {}",
                        format_db_open_diagnostics(db_path, "journal_mode_wal", &wal_err),
                        format_db_open_diagnostics(
                            db_path,
                            "journal_mode_delete_fallback",
                            &delete_err
                        )
                    ))
                })?;
            Ok(())
        }
    }
}

fn db_open_error_with_diagnostics(
    db_path: &Path,
    stage: &str,
    err: &rusqlite::Error,
) -> error::DecapodError {
    error::DecapodError::ValidationError(format_db_open_diagnostics(db_path, stage, err))
}

fn format_db_open_diagnostics(db_path: &Path, stage: &str, err: &rusqlite::Error) -> String {
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

    let sqlite_codes = match err {
        rusqlite::Error::SqliteFailure(code, msg) => format!(
            "sqlite_code={:?} extended_code={} message={}",
            code.code,
            code.extended_code,
            msg.clone().unwrap_or_else(|| "<none>".to_string())
        ),
        _ => format!("sqlite_error={}", err),
    };

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
        "SQLite open/config failed at stage='{}' path='{}' parent='{}' parent_exists={} parent_writable={} db_exists={} db_writable={} TMPDIR={} temp_dir={} temp_dir_writable={} {}; remediation: {}",
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
        sqlite_codes,
        hints.join("; ")
    )
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join(schemas::KNOWLEDGE_DB_NAME)
}

pub fn initialize_knowledge_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = knowledge_db_path(root);
    let parent_dir = db_path.parent().unwrap();
    fs::create_dir_all(parent_dir).map_err(error::DecapodError::IoError)?;

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

fn ensure_knowledge_columns(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("PRAGMA table_info(knowledge)")?;
    let cols_iter = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut cols = std::collections::HashSet::new();
    for c in cols_iter {
        cols.insert(c?);
    }

    let add_col = |name: &str, sql_type: &str, default_expr: &str| -> Result<(), rusqlite::Error> {
        if !cols.contains(name) {
            conn.execute(
                &format!(
                    "ALTER TABLE knowledge ADD COLUMN {} {} DEFAULT {}",
                    name, sql_type, default_expr
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
    root.join(schemas::DECIDE_DB_NAME)
}

pub fn initialize_decide_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = decide_db_path(root);
    let parent_dir = db_path.parent().unwrap();
    fs::create_dir_all(parent_dir).map_err(error::DecapodError::IoError)?;

    let broker = DbBroker::new(root);
    broker.with_conn(&db_path, "decapod", None, "decide.init", |conn| {
        conn.execute(schemas::DECIDE_DB_SCHEMA_META, [])?;
        conn.execute(schemas::DECIDE_DB_SCHEMA_SESSIONS, [])?;
        conn.execute(schemas::DECIDE_DB_SCHEMA_DECISIONS, [])?;
        conn.execute(schemas::DECIDE_DB_INDEX_DECISIONS_SESSION, [])?;
        conn.execute(schemas::DECIDE_DB_INDEX_DECISIONS_TREE, [])?;
        conn.execute(schemas::DECIDE_DB_INDEX_SESSIONS_TREE, [])?;
        conn.execute(schemas::DECIDE_DB_INDEX_SESSIONS_STATUS, [])?;
        Ok(())
    })?;

    Ok(())
}

// Subsystems own their schemas and initialization. Avoid generic "plugin DB" APIs until
// a real extension mechanism exists.
