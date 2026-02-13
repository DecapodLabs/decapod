//! Version detection and automatic migration system.
//!
//! This module handles detecting Decapod version changes and running
//! necessary migrations for schema updates, data transformations, etc.
//!
//! # For AI Agents
//!
//! - **Migrations run automatically**: Version mismatch triggers migration
//! - **Idempotent migrations**: Safe to run multiple times
//! - **Version tracking**: `.decapod/generated/decapod.version` stores current version
//! - **Schema evolution**: Each migration can modify databases, files, etc.

use crate::core::error;
use colored::Colorize;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

/// Current Decapod version from Cargo.toml
pub const DECAPOD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Path to version file relative to .decapod root
const VERSION_FILE: &str = "generated/decapod.version";

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
        // Add migrations here in chronological order by version
        // Each migration should be idempotent (safe to run multiple times)

        // Example: Add index to todo.db for better query performance
        // Migration {
        //     target_version: "0.1.6",
        //     description: "Add status index to todo.db for faster queries",
        //     up: migrate_add_todo_index,
        // },

        // Example: Schema change for a database
        // Migration {
        //     target_version: "0.2.0",
        //     description: "Add priority column to tasks table",
        //     up: migrate_add_task_priority,
        // },
    ]
}

/// Check if migration is needed and run if necessary
pub fn check_and_migrate(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let version_path = decapod_root.join(VERSION_FILE);

    // Read stored version
    let stored_version = if version_path.exists() {
        fs::read_to_string(&version_path)
            .map_err(error::DecapodError::IoError)?
            .trim()
            .to_string()
    } else {
        // No version file - this is either first run or migration from pre-versioning
        String::new()
    };

    // Check if migration needed
    if stored_version == DECAPOD_VERSION {
        // Up to date - no migration needed
        return Ok(());
    }

    // Migration needed!
    run_migrations(decapod_root, &stored_version)?;

    // Update version file
    write_version(decapod_root)?;

    Ok(())
}

/// Run all necessary migrations
fn run_migrations(decapod_root: &Path, from_version: &str) -> Result<(), error::DecapodError> {
    let migrations = all_migrations();

    if migrations.is_empty() {
        // No migrations defined yet - just version bump
        return Ok(());
    }

    println!();
    println!(
        "      {}",
        "╔═══════════════════════════════════════════════╗"
            .bright_yellow()
            .bold()
    );
    println!(
        "      {} {} {}",
        "║".bright_yellow().bold(),
        "🔄 MIGRATION PROTOCOL INITIATED 🔄        "
            .bright_white()
            .bold(),
        "║".bright_yellow().bold()
    );
    println!(
        "      {}",
        "╚═══════════════════════════════════════════════╝"
            .bright_yellow()
            .bold()
    );
    println!();

    if from_version.is_empty() {
        println!(
            "        {} Initializing version tracking",
            "▸".bright_cyan()
        );
    } else {
        println!(
            "        {} Upgrading from {} → {}",
            "▸".bright_cyan(),
            from_version.bright_yellow(),
            DECAPOD_VERSION.bright_green()
        );
    }
    println!();

    let mut applied = 0;
    for migration in migrations {
        // Run migration if we're upgrading past this version
        if should_run_migration(from_version, migration.target_version) {
            println!(
                "        {} {}",
                "●".bright_cyan(),
                migration.description.bright_white()
            );
            (migration.up)(decapod_root)?;
            applied += 1;
        }
    }

    if applied > 0 {
        println!();
        println!(
            "        {} {} migration(s) applied successfully",
            "✓".bright_green(),
            applied
        );
    }

    println!();

    Ok(())
}

/// Determine if a migration should run based on version comparison
fn should_run_migration(from: &str, target: &str) -> bool {
    if from.is_empty() {
        // Fresh install or pre-versioning - don't run old migrations
        return false;
    }

    // Simple string comparison for now - could use semver crate for proper version comparison
    // For now, run all migrations if versions don't match
    from < target
}

/// Write current version to version file
pub fn write_version(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let version_path = decapod_root.join(VERSION_FILE);

    // Ensure generated directory exists
    if let Some(parent) = version_path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }

    fs::write(&version_path, DECAPOD_VERSION).map_err(error::DecapodError::IoError)?;

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
