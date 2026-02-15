//! Version detection and automatic migration system.
//!
//! This module handles detecting Decapod version changes and running
//! necessary migrations for schema updates, data transformations, etc.
//!
//! # For AI Agents
//!
//! - **Migrations run automatically**: Idempotent migrations run on every startup
//! - **Schema evolution**: Each migration can modify databases, files, etc.
//! - **Version management**: Install latest via `cargo install decapod`

use crate::core::error;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

/// Current Decapod version from Cargo.toml
pub const DECAPOD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Migration definition
pub struct Migration {
    /// Version this migration targets (e.g., "0.1.6")
    pub target_version: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Migration function
    pub up: fn(&Path) -> Result<(), error::DecapodError>,
}

/// All migrations in chronological order
pub fn all_migrations() -> Vec<Migration> {
    vec![
        // Reconstruct event log from legacy databases
        Migration {
            target_version: "0.1.7",
            description: "Reconstruct todo event log from database state",
            up: migrate_reconstruct_todo_events,
        },
    ]
}

/// Run any pending migrations (idempotent — safe to call every startup)
pub fn check_and_migrate(decapod_root: &Path) -> Result<(), error::DecapodError> {
    run_migrations(decapod_root)?;
    Ok(())
}

/// Run all idempotent migrations
fn run_migrations(decapod_root: &Path) -> Result<(), error::DecapodError> {
    for migration in all_migrations() {
        // All migrations are idempotent — they check internally if work is needed
        (migration.up)(decapod_root)?;
    }
    Ok(())
}

// Migration functions:

/// Reconstruct todo.events.jsonl from current todo.db state (for legacy migrations)
fn migrate_reconstruct_todo_events(decapod_root: &Path) -> Result<(), error::DecapodError> {
    use serde_json::json;
    use std::io::Write;

    let db_path = decapod_root.join("data/todo.db");
    let events_path = decapod_root.join("data/todo.events.jsonl");

    if !db_path.exists() {
        return Ok(()); // Nothing to migrate
    }

    // Check if events file is empty or missing
    let needs_migration = if events_path.exists() {
        fs::metadata(&events_path)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
    } else {
        true
    };

    if !needs_migration {
        return Ok(()); // Already has events
    }

    let conn = Connection::open(&db_path).map_err(error::DecapodError::RusqliteError)?;

    // Read all tasks from database
    let mut stmt = conn
        .prepare("SELECT id, title, status, created_at FROM tasks ORDER BY created_at")
        .map_err(error::DecapodError::RusqliteError)?;

    let tasks = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // title
                row.get::<_, String>(2)?, // status
                row.get::<_, String>(3)?, // created_at (TEXT in schema)
            ))
        })
        .map_err(error::DecapodError::RusqliteError)?;

    // Create events file
    let mut file = fs::File::create(&events_path).map_err(error::DecapodError::IoError)?;

    // Write task.add event for each task
    for task in tasks {
        let (id, title, status, created_at) = task.map_err(error::DecapodError::RusqliteError)?;

        let event = json!({
            "ts": created_at,
            "event_id": format!("MIGRATION_{}", id),
            "event_type": "task.add",
            "task_id": id,
            "payload": {
                "title": title,
            },
            "actor": "migration",
        });

        writeln!(file, "{}", event).map_err(error::DecapodError::IoError)?;

        // If task is done, add task.done event
        if status == "done" {
            let complete_event = json!({
                "ts": created_at,
                "event_id": format!("MIGRATION_{}_DONE", id),
                "event_type": "task.done",
                "task_id": id,
                "payload": {},
                "actor": "migration",
            });

            writeln!(file, "{}", complete_event).map_err(error::DecapodError::IoError)?;
        }
    }

    Ok(())
}

// Example migration functions (add as needed):

/// Example: Add index to todo.db for better performance
#[allow(dead_code)]
fn migrate_add_todo_index(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let db_path = decapod_root.join("data/todo.db");
    if !db_path.exists() {
        return Ok(()); // Nothing to migrate
    }

    let conn = Connection::open(&db_path).map_err(error::DecapodError::RusqliteError)?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)",
        [],
    )
    .map_err(error::DecapodError::RusqliteError)?;

    Ok(())
}

/// Example: Migrate schema for a database
#[allow(dead_code)]
fn migrate_schema_change(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let db_path = decapod_root.join("data/some.db");
    if !db_path.exists() {
        return Ok(());
    }

    let conn = Connection::open(&db_path).map_err(error::DecapodError::RusqliteError)?;

    // Check if migration already applied
    let has_new_column: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('some_table') WHERE name='new_column'",
        [],
        |row| row.get(0),
    );

    if has_new_column.unwrap_or(0) == 0 {
        // Apply migration
        conn.execute("ALTER TABLE some_table ADD COLUMN new_column TEXT", [])
            .map_err(error::DecapodError::RusqliteError)?;
    }

    Ok(())
}
