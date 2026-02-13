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
use rusqlite::Connection;
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
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(error::DecapodError::RusqliteError)?;
    conn.query_row("PRAGMA journal_mode=WAL;", [], |_| Ok(()))
        .map_err(error::DecapodError::RusqliteError)?;
    conn.execute("PRAGMA foreign_keys=ON;", [])
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(conn)
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
        Ok(())
    })?;

    use colored::Colorize;
    println!("  {} Knowledge database ready", "✓".green());
    Ok(())
}

// Subsystems own their schemas and initialization. Avoid generic "plugin DB" APIs until
// a real extension mechanism exists.
