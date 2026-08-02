//! Version detection and automatic migration system.
//!
//! This module handles detecting Decapod version changes and running
//! necessary migrations for schema updates, data transformations, etc.

use crate::core::db;
use crate::core::error;
use crate::core::events;
use crate::core::schemas;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Current Decapod version from Cargo.toml
pub const DECAPOD_VERSION: &str = env!("CARGO_PKG_VERSION");
const GENERATED_VERSION_COUNTER: &str = "managed/version_counter.json";
const GENERATED_APPLIED_MIGRATIONS: &str = "managed/migrations/applied.json";
const GENERATED_MIGRATION_CATALOG: &str = "managed/migrations/catalog.json";

const LEGACY_LOCAL_DATABASES: &[&str] = &[
    schemas::GOVERNANCE_DB_NAME,
    schemas::MEMORY_DB_NAME,
    schemas::AUTOMATION_DB_NAME,
    schemas::TODO_DB_NAME,
    schemas::LCM_DB_NAME,
    schemas::KNOWLEDGE_DB_NAME,
    schemas::FEDERATION_DB_NAME,
    schemas::DECIDE_DB_NAME,
    schemas::APTITUDE_DB_NAME,
    schemas::CRON_DB_NAME,
    schemas::REFLEX_DB_NAME,
    "broker_dedupe.db",
];

/// Migration definition
pub struct Migration {
    /// Stable migration identifier for durable applied-ledger tracking.
    pub id: &'static str,
    /// Deterministic sequence index used for stable ordering over long migration histories.
    pub sequence: u32,
    /// Logical migration scope (todo/governance/memory/automation/global).
    pub scope: &'static str,
    /// Migration implementation kind (rust/sql/replay).
    pub kind: &'static str,
    /// Optional script path when migration is script-backed.
    pub script_path: Option<&'static str>,
    /// Minimum decapod version where this migration is valid to run.
    pub min_version: &'static str,
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
            id: "todo.events.reconstruct.v001",
            sequence: 100,
            scope: "todo",
            kind: "rust",
            script_path: None,
            min_version: "0.1.7",
            target_version: "0.1.7",
            description: "Reconstruct todo event log from database state",
            up: migrate_reconstruct_todo_events,
        },
        Migration {
            id: "db.consolidate.core_bins.v001",
            sequence: 200,
            scope: "global",
            kind: "rust",
            script_path: None,
            min_version: "0.27.0",
            target_version: "0.27.0",
            description: "Consolidate fragmented databases into core bins",
            up: migrate_consolidate_databases,
        },
        Migration {
            id: "todo.ids.typed.v015",
            sequence: 300,
            scope: "todo",
            kind: "sql",
            script_path: Some("src/decapod/core/sql/todo_task_id_v15_migration.sql"),
            min_version: "0.41.1",
            target_version: "0.41.1",
            description: "Migrate legacy todo IDs to typed <type4>_<16> format",
            up: migrate_todo_ids_to_typed_format,
        },
        Migration {
            id: "todo.one_shot.column.v001",
            sequence: 400,
            scope: "todo",
            kind: "sql",
            script_path: Some("src/decapod/core/sql/todo_one_shot_column_migration.sql"),
            min_version: "0.42.0",
            target_version: "0.42.0",
            description: "Add one_shot column to tasks table for 1-shot task tracking",
            up: migrate_todo_one_shot_column,
        },
        Migration {
            id: "db.consolidate.single_datastore.v001",
            sequence: 500,
            scope: "global",
            kind: "rust",
            script_path: None,
            min_version: "0.89.1",
            target_version: "0.89.1",
            description: "Consolidate every local SQLite store into decapod.db",
            up: migrate_consolidate_single_datastore,
        },
        Migration {
            id: "db.consolidate.schema_fold.v001",
            sequence: 600,
            scope: "global",
            kind: "rust",
            script_path: None,
            min_version: "0.94.0",
            target_version: "0.94.0",
            description: "Fold events, agents, node_edges, task_tags, patterns→meta; drop empty tables (#1126–#1131)",
            up: migrate_schema_fold_v001,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedVersionCounter {
    schema_version: String,
    version_count: u64,
    initialized_with_version: String,
    last_seen_version: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppliedMigrationEntry {
    id: String,
    #[serde(default)]
    sequence: u32,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    script_path: Option<String>,
    min_version: String,
    target_version: String,
    applied_at: String,
    applied_by_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppliedMigrationLedger {
    schema_version: String,
    entries: Vec<AppliedMigrationEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct MigrationCatalogEntry {
    id: String,
    sequence: u32,
    scope: String,
    kind: String,
    script_path: Option<String>,
    min_version: String,
    target_version: String,
    description: String,
}

#[derive(Debug, Clone, Serialize)]
struct MigrationCatalog {
    schema_version: String,
    generated_at: String,
    latest_sequence: u32,
    count: usize,
    migrations: Vec<MigrationCatalogEntry>,
}

#[derive(Debug, Clone)]
pub struct DbSchemaVersionCheck {
    pub db_name: String,
    pub expected_version: u32,
    pub actual_version: Option<u32>,
    pub exists: bool,
}

/// Run any pending migrations (idempotent — safe to call every startup)
pub fn check_and_migrate(decapod_root: &Path) -> Result<(), error::DecapodError> {
    reconcile_post_consolidation_artifacts(decapod_root)?;
    run_migrations(decapod_root)?;
    Ok(())
}

pub fn check_and_migrate_with_backup<F>(
    decapod_root: &Path,
    verify: F,
) -> Result<(), error::DecapodError>
where
    F: FnOnce(&Path) -> Result<(), error::DecapodError>,
{
    let data_root = decapod_root.join("data");
    reconcile_post_consolidation_artifacts(decapod_root)?;
    if !schema_upgrade_pending(&data_root)? {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        return Ok(());
    }

    checkpoint_legacy_databases(&data_root)?;
    let Some(backup_dir) = create_data_backup(&data_root)? else {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        return Ok(());
    };

    let result = (|| -> Result<(), error::DecapodError> {
        run_migrations(decapod_root)?;
        verify(&data_root)?;
        Ok(())
    })();

    if let Err(err) = result {
        restore_data_backup(&data_root, &backup_dir)?;
        let _ = fs::remove_dir_all(&backup_dir);
        return Err(error::DecapodError::ValidationError(format!(
            "Migration failed; restored .decapod/data backup from {}: {}",
            backup_dir.display(),
            err
        )));
    }

    fs::remove_dir_all(&backup_dir).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn reconcile_post_consolidation_artifacts(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let applied = load_applied_migrations(decapod_root)?;
    if !applied
        .entries
        .iter()
        .any(|entry| entry.id == "db.consolidate.single_datastore.v001")
    {
        return Ok(());
    }
    let data_root = decapod_root.join("data");
    if !LEGACY_LOCAL_DATABASES
        .iter()
        .any(|name| legacy_database_artifact_exists(&data_root, name))
    {
        return Ok(());
    }

    let target_path = data_root.join(schemas::LOCAL_DB_NAME);
    if !target_path.is_file() {
        return Err(error::DecapodError::ValidationError(
            "CONSOLIDATED_STORE_MISSING: migration ledger proves single-datastore consolidation but decapod.db is absent"
                .to_string(),
        ));
    }
    let target = db::db_connect(&target_path.to_string_lossy())?;
    events::mark_previously_consolidated_legacy_inputs(
        &data_root,
        &target,
        "db.consolidate.single_datastore.v001",
    )?;
    drop(target);

    // Older binaries may recreate a retired per-subsystem database after the
    // original consolidation. Re-run the existing idempotent copier so newer
    // rows reach decapod.db before the retired files are removed.
    migrate_consolidate_single_datastore(decapod_root)
}

fn schema_upgrade_pending(data_root: &Path) -> Result<bool, error::DecapodError> {
    if LEGACY_LOCAL_DATABASES
        .iter()
        .any(|name| legacy_database_artifact_exists(data_root, name))
    {
        return Ok(true);
    }
    let todo_db = data_root.join(schemas::LOCAL_DB_NAME);
    if !todo_db.exists() {
        return Ok(true);
    }
    let conn = db::db_connect(&todo_db.to_string_lossy())?;
    let version_res: Result<String, _> = conn.query_row(
        "SELECT value FROM meta WHERE namespace = ?1 AND key = 'schema_version'",
        [schemas::TODO_META_NAMESPACE],
        |row| row.get(0),
    );
    let current_version = version_res
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(current_version < schemas::TODO_SCHEMA_VERSION)
}

fn legacy_database_artifact_exists(data_root: &Path, name: &str) -> bool {
    data_root.join(name).exists()
        || data_root.join(format!("{name}-wal")).exists()
        || data_root.join(format!("{name}-shm")).exists()
}

fn checkpoint_legacy_databases(data_root: &Path) -> Result<(), error::DecapodError> {
    for name in LEGACY_LOCAL_DATABASES
        .iter()
        .copied()
        .chain(std::iter::once(schemas::LOCAL_DB_NAME))
    {
        let path = data_root.join(name);
        if !path.is_file() {
            continue;
        }
        let conn = Connection::open(&path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    }
    Ok(())
}

fn create_data_backup(data_root: &Path) -> Result<Option<std::path::PathBuf>, error::DecapodError> {
    if !data_root.exists() {
        return Ok(None);
    }
    let backup_dir = data_root.join(format!(
        ".migration_backup_{}_{}",
        DECAPOD_VERSION.replace('.', "_"),
        crate::core::ulid::new_ulid()
    ));
    fs::create_dir_all(&backup_dir).map_err(error::DecapodError::IoError)?;

    let copy_result = (|| -> Result<(), error::DecapodError> {
        for entry in fs::read_dir(data_root).map_err(error::DecapodError::IoError)? {
            let entry = entry.map_err(error::DecapodError::IoError)?;
            let path = entry.path();
            if path == backup_dir {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            copy_data_entry(&path, &backup_dir.join(name))?;
        }
        Ok(())
    })();
    if let Err(err) = copy_result {
        let _ = fs::remove_dir_all(&backup_dir);
        return Err(err);
    }
    Ok(Some(backup_dir))
}

fn restore_data_backup(data_root: &Path, backup_dir: &Path) -> Result<(), error::DecapodError> {
    for entry in fs::read_dir(data_root).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        if entry.path() == backup_dir {
            continue;
        }
        remove_data_entry(&entry.path())?;
    }
    for entry in fs::read_dir(backup_dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let name = entry.file_name();
        copy_data_entry(&entry.path(), &data_root.join(name))?;
    }
    Ok(())
}

fn copy_data_entry(source: &Path, target: &Path) -> Result<(), error::DecapodError> {
    let metadata = fs::symlink_metadata(source).map_err(error::DecapodError::IoError)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() && !metadata.is_file() {
        // Runtime sockets and other special entries are process-local state,
        // not migration data. Preserve them in place without following or
        // copying them into the backup tree.
        return Ok(());
    }
    if metadata.is_dir() {
        fs::create_dir_all(target).map_err(error::DecapodError::IoError)?;
        for entry in fs::read_dir(source).map_err(error::DecapodError::IoError)? {
            let entry = entry.map_err(error::DecapodError::IoError)?;
            copy_data_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        fs::copy(source, target).map_err(error::DecapodError::IoError)?;
    } else {
        return Err(error::DecapodError::ValidationError(format!(
            "Cannot migrate unsupported data artifact: {}",
            source.display()
        )));
    }
    Ok(())
}

fn remove_data_entry(path: &Path) -> Result<(), error::DecapodError> {
    let metadata = fs::symlink_metadata(path).map_err(error::DecapodError::IoError)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() && !metadata.is_file() {
        return Ok(());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(error::DecapodError::IoError)?;
    } else {
        fs::remove_file(path).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

/// Run all idempotent migrations
fn run_migrations(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let mut migrations = all_migrations();
    migrations.sort_by_key(|m| m.sequence);
    validate_migration_plan(&migrations)?;
    touch_generated_version_counter(decapod_root)?;
    touch_generated_migration_catalog(decapod_root, &migrations)?;
    let mut applied = load_applied_migrations(decapod_root)?;
    let mut applied_ids: HashSet<String> = applied.entries.iter().map(|e| e.id.clone()).collect();
    let single_datastore_was_previously_consolidated =
        applied_ids.contains("db.consolidate.single_datastore.v001");
    for migration in migrations {
        if !version_gte(DECAPOD_VERSION, migration.min_version) {
            continue;
        }
        if !version_gte(DECAPOD_VERSION, migration.target_version) {
            continue;
        }
        if applied_ids.contains(migration.id) {
            continue;
        }
        (migration.up)(decapod_root)?;
        applied.entries.push(AppliedMigrationEntry {
            id: migration.id.to_string(),
            sequence: migration.sequence,
            scope: migration.scope.to_string(),
            kind: migration.kind.to_string(),
            script_path: migration.script_path.map(|s| s.to_string()),
            min_version: migration.min_version.to_string(),
            target_version: migration.target_version.to_string(),
            applied_at: crate::core::time::now_epoch_z(),
            applied_by_version: DECAPOD_VERSION.to_string(),
        });
        applied_ids.insert(migration.id.to_string());
        store_applied_migrations(decapod_root, &applied)?;
    }
    reconcile_canonical_event_tables(decapod_root, single_datastore_was_previously_consolidated)?;
    Ok(())
}

/// Keep canonical SQLite event tables complete. JSONL import is sealed inside
/// the historical single-datastore migration and never runs on this steady-state path.
fn reconcile_canonical_event_tables(
    decapod_root: &Path,
    single_datastore_was_previously_consolidated: bool,
) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    if !db_path.exists() {
        return Ok(());
    }
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    events::ensure_tables(&conn)?;
    if single_datastore_was_previously_consolidated {
        events::mark_previously_consolidated_legacy_inputs(
            &data_root,
            &conn,
            "db.consolidate.single_datastore.v001",
        )?;
    }
    migrate_task_events_to_canonical_stream(&conn)?;
    Ok(())
}

pub fn check_versioned_db_schema_expectations(
    data_root: &Path,
) -> Result<Vec<DbSchemaVersionCheck>, error::DecapodError> {
    let expectations = vec![(schemas::LOCAL_DB_NAME, schemas::TODO_SCHEMA_VERSION)];
    let mut checks = Vec::with_capacity(expectations.len());
    for (db_name, expected) in expectations {
        let db_path = data_root.join(db_name);
        if !db_path.exists() {
            checks.push(DbSchemaVersionCheck {
                db_name: db_name.to_string(),
                expected_version: expected,
                actual_version: None,
                exists: false,
            });
            continue;
        }
        let conn = db::db_connect(&db_path.to_string_lossy())?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE namespace = ?1 AND key = 'schema_version'",
                [schemas::TODO_META_NAMESPACE],
                |row| row.get(0),
            )
            .optional()
            .map_err(error::DecapodError::RusqliteError)?;
        let actual = raw.and_then(|s| s.parse::<u32>().ok());
        checks.push(DbSchemaVersionCheck {
            db_name: db_name.to_string(),
            expected_version: expected,
            actual_version: actual,
            exists: true,
        });
    }
    Ok(checks)
}

fn parse_version(v: &str) -> [u64; 3] {
    let mut out = [0u64; 3];
    for (idx, part) in v.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[idx] = digits.parse::<u64>().unwrap_or(0);
    }
    out
}

fn validate_migration_plan(migrations: &[Migration]) -> Result<(), error::DecapodError> {
    let mut ids = HashSet::new();
    let mut sequences = HashSet::new();
    let mut prev = 0u32;
    for migration in migrations {
        if !ids.insert(migration.id) {
            return Err(error::DecapodError::ValidationError(format!(
                "Duplicate migration id detected: {}",
                migration.id
            )));
        }
        if !sequences.insert(migration.sequence) {
            return Err(error::DecapodError::ValidationError(format!(
                "Duplicate migration sequence detected: {}",
                migration.sequence
            )));
        }
        if migration.sequence <= prev {
            return Err(error::DecapodError::ValidationError(format!(
                "Migration sequence is not strictly increasing at {} ({} <= {})",
                migration.id, migration.sequence, prev
            )));
        }
        if !version_gte(migration.target_version, migration.min_version) {
            return Err(error::DecapodError::ValidationError(format!(
                "Migration {} has invalid version range min={} target={}",
                migration.id, migration.min_version, migration.target_version
            )));
        }
        prev = migration.sequence;
    }
    Ok(())
}

fn version_gte(left: &str, right: &str) -> bool {
    parse_version(left) >= parse_version(right)
}

fn touch_generated_version_counter(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let path = decapod_root.join(GENERATED_VERSION_COUNTER);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let now = crate::core::time::now_epoch_z();
    let mut counter = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
        serde_json::from_str::<GeneratedVersionCounter>(&raw).unwrap_or(GeneratedVersionCounter {
            schema_version: "1.0.0".to_string(),
            version_count: 1,
            initialized_with_version: DECAPOD_VERSION.to_string(),
            last_seen_version: DECAPOD_VERSION.to_string(),
            updated_at: now.clone(),
        })
    } else {
        GeneratedVersionCounter {
            schema_version: "1.0.0".to_string(),
            version_count: 1,
            initialized_with_version: DECAPOD_VERSION.to_string(),
            last_seen_version: DECAPOD_VERSION.to_string(),
            updated_at: now.clone(),
        }
    };

    if counter.last_seen_version != DECAPOD_VERSION {
        counter.version_count = counter.version_count.saturating_add(1);
        counter.last_seen_version = DECAPOD_VERSION.to_string();
    }
    counter.updated_at = now;
    let body = serde_json::to_string_pretty(&counter)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn load_applied_migrations(
    decapod_root: &Path,
) -> Result<AppliedMigrationLedger, error::DecapodError> {
    let path = decapod_root.join(GENERATED_APPLIED_MIGRATIONS);
    if !path.exists() {
        return Ok(AppliedMigrationLedger {
            schema_version: "1.0.0".to_string(),
            entries: vec![],
        });
    }
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    let mut ledger = serde_json::from_str::<AppliedMigrationLedger>(&raw).unwrap_or_default();
    if ledger.schema_version.is_empty() {
        ledger.schema_version = "1.0.0".to_string();
    }
    Ok(ledger)
}

fn store_applied_migrations(
    decapod_root: &Path,
    ledger: &AppliedMigrationLedger,
) -> Result<(), error::DecapodError> {
    let path = decapod_root.join(GENERATED_APPLIED_MIGRATIONS);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let body = serde_json::to_string_pretty(ledger)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn touch_generated_migration_catalog(
    decapod_root: &Path,
    migrations: &[Migration],
) -> Result<(), error::DecapodError> {
    let path = decapod_root.join(GENERATED_MIGRATION_CATALOG);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let entries = migrations
        .iter()
        .map(|m| MigrationCatalogEntry {
            id: m.id.to_string(),
            sequence: m.sequence,
            scope: m.scope.to_string(),
            kind: m.kind.to_string(),
            script_path: m.script_path.map(|s| s.to_string()),
            min_version: m.min_version.to_string(),
            target_version: m.target_version.to_string(),
            description: m.description.to_string(),
        })
        .collect::<Vec<_>>();
    let latest_sequence = migrations.iter().map(|m| m.sequence).max().unwrap_or(0);
    let catalog = MigrationCatalog {
        schema_version: "1.0.0".to_string(),
        generated_at: crate::core::time::now_epoch_z(),
        latest_sequence,
        count: entries.len(),
        migrations: entries,
    };
    let body = serde_json::to_string_pretty(&catalog)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
    Ok(())
}

// Migration functions:

/// Reconstruct the canonical todo event table from current todo.db state.
/// JSONL is a read-only migration input; this migration never creates it.
fn migrate_reconstruct_todo_events(decapod_root: &Path) -> Result<(), error::DecapodError> {
    use serde_json::json;

    let db_path = decapod_root.join("data/todo.db");

    if !db_path.exists() {
        return Ok(()); // Nothing to migrate
    }

    let source = db::db_connect(&db_path.to_string_lossy())?;
    let target_path = decapod_root.join("data").join(schemas::LOCAL_DB_NAME);
    let target = db::db_connect(&target_path.to_string_lossy())?;
    events::ensure_tables(&target)?;

    // Read all tasks from database
    let mut stmt = source
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

        events::append_on_conn(&target, events::TODO, &event)?;

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

            events::append_on_conn(&target, events::TODO, &complete_event)?;
        }
    }

    Ok(())
}

fn migrate_consolidate_databases(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    if !data_root.exists() {
        return Ok(());
    }

    // 1. Consolidate Governance Bin (health, policy, feedback, archive)
    let gov_path = data_root.join(schemas::GOVERNANCE_DB_NAME);
    let gov_conn = db::db_connect(&gov_path.to_string_lossy())?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_CLAIMS)?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS)?;
    gov_conn.execute_batch(schemas::HEALTH_DB_SCHEMA_HEALTH_CACHE)?;
    gov_conn.execute_batch(schemas::POLICY_DB_SCHEMA_APPROVALS)?;
    gov_conn.execute_batch(schemas::POLICY_DB_SCHEMA_INDEX)?;
    gov_conn.execute_batch(schemas::FEEDBACK_DB_SCHEMA)?;
    gov_conn.execute_batch(schemas::ARCHIVE_DB_SCHEMA)?;

    migrate_table(&data_root, "health.db", &gov_conn, "claims")?;
    migrate_table(&data_root, "health.db", &gov_conn, "proof_events")?;
    migrate_table(&data_root, "health.db", &gov_conn, "health_cache")?;
    migrate_table(&data_root, "policy.db", &gov_conn, "approvals")?;
    migrate_table(&data_root, "feedback.db", &gov_conn, "feedback")?;
    migrate_table(&data_root, "archive.db", &gov_conn, "archives")?;

    // 2. Consolidate Memory Bin (knowledge, federation, decisions, aptitude)
    let mem_path = data_root.join(schemas::MEMORY_DB_NAME);
    let mem_conn = db::db_connect(&mem_path.to_string_lossy())?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_META)?;
    mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_NODES)?;
    // Intermediate multi-bin shape; schema_fold later migrates to node_edges + events.
    #[allow(deprecated)]
    {
        mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_SOURCES)?;
        mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EDGES)?;
        mem_conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EVENTS)?;
    }

    migrate_table(&data_root, "federation.db", &mem_conn, "nodes")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "sources")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "edges")?;
    migrate_table(&data_root, "federation.db", &mem_conn, "federation_events")?;

    // Legacy knowledge to nodes migration (simplified)
    let knowledge_db = data_root.join("knowledge.db");
    if knowledge_db.exists() {
        let k_conn = db::db_connect(&knowledge_db.to_string_lossy())?;
        // Guard against concurrent processes that may have created the file
        // but not yet populated the schema (race between Connection::open and
        // CREATE TABLE in initialize_knowledge_db).
        let has_table: bool = k_conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='knowledge'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if has_table {
            let mut stmt = k_conn
                .prepare("SELECT id, title, content, provenance, created_at FROM knowledge")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for r in rows {
                let (id, title, content, prov, ts) = r?;
                mem_conn.execute("INSERT OR IGNORE INTO nodes(id, node_type, title, body, created_at, updated_at, dir_path, scope) VALUES(?1, 'observation', ?2, ?3, ?4, ?4, '', 'repo')", rusqlite::params![id, title, content, ts])?;
                mem_conn.execute("INSERT OR IGNORE INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)", rusqlite::params![crate::core::ulid::new_ulid(), id, prov, ts])?;
            }
        }
    }

    // 3. Consolidate Automation Bin (cron, reflex)
    let auto_path = data_root.join(schemas::AUTOMATION_DB_NAME);
    let auto_conn = db::db_connect(&auto_path.to_string_lossy())?;
    auto_conn.execute_batch(schemas::CRON_DB_SCHEMA)?;
    auto_conn.execute_batch(schemas::REFLEX_DB_SCHEMA)?;

    migrate_table(&data_root, "cron.db", &auto_conn, "cron_jobs")?;
    migrate_table(&data_root, "reflex.db", &auto_conn, "reflexes")?;

    // Cleanup legacy and backup files
    let legacy = [
        "health.db",
        "policy.db",
        "feedback.db",
        "archive.db",
        "knowledge.db",
        "federation.db",
        "decisions.db",
        "aptitude.db",
        "cron.db",
        "reflex.db",
    ];
    for f in legacy {
        let p = data_root.join(f);
        if p.exists() {
            let _ = fs::remove_file(&p);
        }
        let bak = data_root.join(format!("{f}.bak"));
        if bak.exists() {
            let _ = fs::remove_file(&bak);
        }
    }

    Ok(())
}

/// Move the four historical bins and the remaining legacy SQLite stores into
/// one local datastore. This migration deliberately copies into a fully
/// initialized target first, then removes source files only after every copy
/// succeeds. It is safe to rerun because all inserts are idempotent.
fn migrate_consolidate_single_datastore(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    if !data_root.exists() {
        return Ok(());
    }

    let target_path = data_root.join(schemas::LOCAL_DB_NAME);
    let target = db::db_connect(&target_path.to_string_lossy())?;
    initialize_single_datastore_schema(&target)?;
    // Import legacy append-only logs before removing legacy SQLite stores.
    // The source JSONL files remain untouched as an operator-visible archive.
    events::import_legacy_jsonl(&data_root, &target)?;

    let sources = [
        (
            schemas::GOVERNANCE_DB_NAME,
            &[
                "claims",
                "proof_events",
                "health_cache",
                "approvals",
                "feedback",
                "archives",
                "obligations",
                "obligation_edges",
            ][..],
        ),
        (
            schemas::MEMORY_DB_NAME,
            &[
                "nodes",
                "sources",
                "edges",
                "federation_events",
                "knowledge",
                "sessions",
                "decisions",
                "preferences",
                "patterns",
                "observations",
                "consolidations",
                "agent_prompts",
            ][..],
        ),
        (schemas::AUTOMATION_DB_NAME, &["cron_jobs", "reflexes"][..]),
        (
            schemas::TODO_DB_NAME,
            &[
                "tasks",
                "task_events",
                "task_verification",
                "categories",
                "agent_category_claims",
                "agent_presence",
                "agent_trust",
                "risk_zones",
                "task_owners",
                "task_dependencies",
                "agent_expertise",
            ][..],
        ),
        (schemas::LCM_DB_NAME, &["originals_index", "summaries"][..]),
        ("knowledge.db", &["knowledge"][..]),
        (
            schemas::FEDERATION_DB_NAME,
            &["nodes", "sources", "edges", "federation_events"][..],
        ),
        (schemas::DECIDE_DB_NAME, &["sessions", "decisions"][..]),
        (
            schemas::APTITUDE_DB_NAME,
            &[
                "preferences",
                "patterns",
                "observations",
                "consolidations",
                "agent_prompts",
            ][..],
        ),
        (schemas::CRON_DB_NAME, &["cron_jobs"][..]),
        (schemas::REFLEX_DB_NAME, &["reflexes"][..]),
        ("broker_dedupe.db", &["request_dedupe"][..]),
    ];

    for (source_name, tables) in sources {
        if source_name == schemas::LOCAL_DB_NAME {
            continue;
        }
        let source_path = data_root.join(source_name);
        if !source_path.exists() {
            continue;
        }
        for table in tables {
            migrate_table(&data_root, source_name, &target, table)?;
        }
        migrate_legacy_meta(&data_root, source_name, &target)?;
    }
    migrate_task_events_to_canonical_stream(&target)?;

    for name in LEGACY_LOCAL_DATABASES {
        let path = data_root.join(name);
        if path.exists() {
            fs::remove_file(&path).map_err(error::DecapodError::IoError)?;
        }
        for suffix in ["-wal", "-shm"] {
            let sidecar = data_root.join(format!("{name}{suffix}"));
            if sidecar.exists() {
                fs::remove_file(sidecar).map_err(error::DecapodError::IoError)?;
            }
        }
    }
    Ok(())
}

fn migrate_task_events_to_canonical_stream(target: &Connection) -> Result<(), error::DecapodError> {
    if !table_exists(target, "task_events")? || !table_exists(target, "todo_events")? {
        return Ok(());
    }
    let mut stmt = target.prepare(
        "SELECT event_id, ts, event_type, task_id, payload, actor
         FROM task_events ORDER BY seq ASC, ts ASC, event_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut events_to_copy = Vec::new();
    for row in rows {
        let (event_id, ts, event_type, task_id, payload, actor) = row?;
        events_to_copy.push(serde_json::json!({
            "event_id": event_id,
            "ts": ts,
            "event_type": event_type,
            "task_id": task_id,
            "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::String(payload)),
            "actor": actor,
        }));
    }
    drop(stmt);
    for event in events_to_copy {
        events::append_on_conn(target, events::TODO, &event)?;
    }
    Ok(())
}

fn initialize_single_datastore_schema(conn: &Connection) -> Result<(), error::DecapodError> {
    // Consolidated surface (#1126–#1131): one events table, agents row, node_edges,
    // task_tags; empty tables no longer bootstrapped by default.
    for schema in [
        schemas::HEALTH_DB_SCHEMA_CLAIMS,
        schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS,
        schemas::POLICY_DB_SCHEMA_APPROVALS,
        schemas::FEEDBACK_DB_SCHEMA,
        schemas::ARCHIVE_DB_SCHEMA,
        schemas::GOVERNANCE_DB_SCHEMA_OBLIGATIONS,
        schemas::GOVERNANCE_DB_SCHEMA_OBLIGATION_EDGES,
        schemas::MEMORY_DB_SCHEMA_META,
        schemas::MEMORY_DB_SCHEMA_NODES,
        schemas::MEMORY_DB_SCHEMA_NODE_EDGES,
        schemas::DECIDE_DB_SCHEMA_SESSIONS,
        schemas::DECIDE_DB_SCHEMA_DECISIONS,
        schemas::TODO_DB_SCHEMA_META,
        schemas::TODO_DB_SCHEMA_TASKS,
        schemas::TODO_DB_SCHEMA_TASK_VERIFICATION,
        schemas::TODO_DB_SCHEMA_TASK_TAGS,
        schemas::TODO_DB_SCHEMA_CATEGORIES,
        schemas::TODO_DB_SCHEMA_RISK_ZONES,
        schemas::TODO_DB_SCHEMA_TASK_OWNERS,
        schemas::TODO_DB_SCHEMA_TASK_DEPENDENCIES,
        schemas::AGENTS_TABLE_SCHEMA,
        schemas::KNOWLEDGE_DB_SCHEMA,
        schemas::APTITUDE_DB_SCHEMA_PREFERENCES,
        schemas::APTITUDE_DB_SCHEMA_AGENT_PROMPTS,
    ] {
        conn.execute_batch(schema)?;
    }
    for index in [
        schemas::POLICY_DB_SCHEMA_INDEX,
        schemas::MEMORY_DB_INDEX_NODES_TYPE,
        schemas::MEMORY_DB_INDEX_NODES_STATUS,
        schemas::MEMORY_DB_INDEX_NODES_SCOPE,
        schemas::MEMORY_DB_INDEX_NODES_PRIORITY,
        schemas::MEMORY_DB_INDEX_NODES_UPDATED,
        schemas::DECIDE_DB_INDEX_DECISIONS_SESSION,
        schemas::DECIDE_DB_INDEX_DECISIONS_TREE,
        schemas::DECIDE_DB_INDEX_SESSIONS_TREE,
        schemas::DECIDE_DB_INDEX_SESSIONS_STATUS,
        schemas::TODO_DB_SCHEMA_INDEX_STATUS,
        schemas::TODO_DB_SCHEMA_INDEX_SCOPE,
        schemas::TODO_DB_SCHEMA_INDEX_DIR,
        schemas::TODO_DB_SCHEMA_INDEX_HASH,
        schemas::TODO_DB_SCHEMA_INDEX_VERIFICATION_STATUS,
        schemas::TODO_DB_SCHEMA_INDEX_CATEGORY_NAME,
        schemas::TODO_DB_SCHEMA_INDEX_RISK_ZONES_NAME,
        schemas::TODO_DB_SCHEMA_INDEX_TASK_OWNERS_TASK,
        schemas::TODO_DB_SCHEMA_INDEX_TASK_DEPS_TASK,
        schemas::TODO_DB_SCHEMA_INDEX_TASK_DEPS_DEPENDS_ON,
        schemas::APTITUDE_DB_SCHEMA_INDEX_PREF_CATEGORY,
        schemas::APTITUDE_DB_SCHEMA_INDEX_PREF_KEY,
        schemas::APTITUDE_DB_SCHEMA_INDEX_PREF_ACCESS,
        schemas::APTITUDE_DB_SCHEMA_INDEX_PROMPT_CONTEXT,
    ] {
        conn.execute_batch(index)?;
    }
    events::ensure_tables(conn)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS request_dedupe (
            request_id TEXT PRIMARY KEY,
            payload_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            commit_marker TEXT,
            result_envelope TEXT NOT NULL,
            retry_after_ms_hint INTEGER,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_request_dedupe_created_at ON request_dedupe(created_at);",
    )?;
    Ok(())
}

/// Schema fold for #1126–#1131: unified events, agents, node_edges, task_tags, patterns in meta.
fn migrate_schema_fold_v001(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    let db_path = data_root.join(schemas::LOCAL_DB_NAME);
    if !db_path.exists() {
        return Ok(());
    }
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    // Events unification + historical stream table drop.
    events::ensure_tables(&conn)?;

    // Agents table (#1129)
    conn.execute_batch(schemas::AGENTS_TABLE_SCHEMA)?;
    if table_exists(&conn, "agent_presence")? {
        conn.execute(
            "INSERT OR IGNORE INTO agents(agent_id, last_seen, status, updated_at, trust_level, expertise_json, category_claims_json)
             SELECT agent_id, last_seen, status, updated_at, 'basic', '[]', '[]' FROM agent_presence",
            [],
        )?;
    }
    if table_exists(&conn, "agent_trust")? {
        conn.execute(
            "UPDATE agents SET
               trust_level = COALESCE((SELECT trust_level FROM agent_trust t WHERE t.agent_id = agents.agent_id), trust_level),
               trust_granted_at = (SELECT granted_at FROM agent_trust t WHERE t.agent_id = agents.agent_id),
               trust_granted_by = COALESCE((SELECT granted_by FROM agent_trust t WHERE t.agent_id = agents.agent_id), trust_granted_by)
             WHERE EXISTS (SELECT 1 FROM agent_trust t WHERE t.agent_id = agents.agent_id)",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO agents(agent_id, trust_level, trust_granted_at, trust_granted_by, last_seen, status, expertise_json, category_claims_json, updated_at)
             SELECT agent_id, trust_level, granted_at, granted_by, NULL, 'active', '[]', '[]', COALESCE(granted_at, '')
             FROM agent_trust",
            [],
        )?;
    }
    if table_exists(&conn, "agent_expertise")? {
        let mut stmt = conn.prepare(
            "SELECT agent_id, id, category, expertise_level, claimed_at, updated_at FROM agent_expertise ORDER BY agent_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut by_agent: std::collections::BTreeMap<String, Vec<serde_json::Value>> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (agent_id, id, category, level, claimed_at, updated_at) = row?;
            by_agent
                .entry(agent_id)
                .or_default()
                .push(serde_json::json!({
                    "id": id,
                    "category": category,
                    "level": level,
                    "claimed_at": claimed_at,
                    "updated_at": updated_at,
                }));
        }
        for (agent_id, entries) in by_agent {
            let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());
            conn.execute(
                "INSERT INTO agents(agent_id, trust_level, expertise_json, category_claims_json, updated_at, status)
                 VALUES(?1, 'basic', ?2, '[]', '', 'active')
                 ON CONFLICT(agent_id) DO UPDATE SET expertise_json = excluded.expertise_json",
                rusqlite::params![agent_id, json],
            )?;
        }
    }
    if table_exists(&conn, "agent_category_claims")? {
        let mut stmt = conn.prepare(
            "SELECT agent_id, id, category, claimed_at, updated_at FROM agent_category_claims ORDER BY agent_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut by_agent: std::collections::BTreeMap<String, Vec<serde_json::Value>> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (agent_id, id, category, claimed_at, updated_at) = row?;
            by_agent
                .entry(agent_id)
                .or_default()
                .push(serde_json::json!({
                    "id": id,
                    "category": category,
                    "claimed_at": claimed_at,
                    "updated_at": updated_at,
                }));
        }
        for (agent_id, entries) in by_agent {
            let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());
            conn.execute(
                "INSERT INTO agents(agent_id, trust_level, expertise_json, category_claims_json, updated_at, status)
                 VALUES(?1, 'basic', '[]', ?2, '', 'active')
                 ON CONFLICT(agent_id) DO UPDATE SET category_claims_json = excluded.category_claims_json",
                rusqlite::params![agent_id, json],
            )?;
        }
    }
    for table in [
        "agent_presence",
        "agent_trust",
        "agent_expertise",
        "agent_category_claims",
    ] {
        if table_exists(&conn, table)? {
            conn.execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
        }
    }

    // Knowledge graph: edges → node_edges; sources → node_edges edge_type='source' (#1128)
    conn.execute_batch(schemas::MEMORY_DB_SCHEMA_NODE_EDGES)?;
    if table_exists(&conn, "edges")? {
        conn.execute(
            "INSERT OR IGNORE INTO node_edges(id, source_id, target_id, edge_type, metadata, created_at, actor)
             SELECT id, source_id, target_id, edge_type, '{}', created_at, COALESCE(actor, 'decapod') FROM edges",
            [],
        )?;
        conn.execute("DROP TABLE IF EXISTS edges", [])?;
    }
    if table_exists(&conn, "sources")? {
        conn.execute(
            "INSERT OR IGNORE INTO node_edges(id, source_id, target_id, edge_type, metadata, created_at, actor)
             SELECT id, node_id, node_id, 'source',
                    json_object('source', source),
                    created_at, 'decapod'
             FROM sources",
            [],
        )?;
        conn.execute("DROP TABLE IF EXISTS sources", [])?;
    }
    // Empty graph satellites superseded by nodes + node_edges (keep originals_index for LCM)
    if table_exists(&conn, "summaries")? {
        conn.execute("DROP TABLE IF EXISTS summaries", [])?;
    }

    // Task tags (#1130) — dual-write path; denormalized columns remain for one release
    conn.execute_batch(schemas::TODO_DB_SCHEMA_TASK_TAGS)?;
    if table_exists(&conn, "tasks")? && table_has_column(&conn, "tasks", "tags")? {
        let mut stmt =
            conn.prepare("SELECT id, tags FROM tasks WHERE tags IS NOT NULL AND tags != ''")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, tags) = row?;
            for tag in tags.split([',', ' ']) {
                let tag = tag.trim();
                if tag.is_empty() {
                    continue;
                }
                conn.execute(
                    "INSERT OR IGNORE INTO task_tags(task_id, tag) VALUES(?1, ?2)",
                    rusqlite::params![id, tag],
                )?;
            }
        }
    }

    // Patterns → meta (#1131)
    if table_exists(&conn, "patterns")? {
        let mut stmt = conn.prepare(
            "SELECT id, name, category, regex_pattern, preference_category, preference_key, description, created_at FROM patterns",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        for row in rows {
            let (id, name, category, regex, pref_cat, pref_key, desc, created) = row?;
            let value = serde_json::json!({
                "id": id,
                "name": name,
                "category": category,
                "regex_pattern": regex,
                "preference_category": pref_cat,
                "preference_key": pref_key,
                "description": desc,
                "created_at": created,
            });
            conn.execute(
                "INSERT OR REPLACE INTO meta(namespace, key, value) VALUES('aptitude', ?1, ?2)",
                rusqlite::params![format!("pattern:{name}"), value.to_string()],
            )?;
        }
        conn.execute("DROP TABLE IF EXISTS patterns", [])?;
    }

    // Drop empty / consolidated-away tables (#1126). Plugins that still expose
    // CLI surfaces recreate their tables lazily on first use.
    for table in [
        // consolidated event streams
        "task_events",
        "federation_events",
        "broker_events",
        "todo_events",
        "external_actions_events",
        "traces_events",
        "verification_events",
        // note: do not drop health `proof_events` (claim proofs); it is not a stream table
        "knowledge_events",
        "lcm_events",
        "map_events",
        "watcher_events",
        // empty aptitude satellites (patterns already folded)
        "consolidations",
        "observations",
        // empty cache / graph satellites (originals_index stays for LCM)
        "summaries",
        "health_cache",
        // `knowledge` stays: plugin surface still writes entries; graph is nodes+node_edges.
    ] {
        let _ = conn.execute(&format!("DROP TABLE IF EXISTS {table}"), []);
    }

    // Ensure consolidated schema present for new code paths.
    initialize_single_datastore_schema(&conn)?;
    Ok(())
}

fn migrate_legacy_meta(
    data_root: &Path,
    source_name: &str,
    target: &Connection,
) -> Result<(), error::DecapodError> {
    let source_path = data_root.join(source_name);
    let source = db::db_connect(&source_path.to_string_lossy())?;
    if !table_exists(&source, "meta")? {
        return Ok(());
    }
    let namespace = match source_name {
        schemas::TODO_DB_NAME => schemas::TODO_META_NAMESPACE,
        schemas::LCM_DB_NAME => schemas::LCM_META_NAMESPACE,
        schemas::FEDERATION_DB_NAME => schemas::FEDERATION_META_NAMESPACE,
        _ => schemas::MEMORY_META_NAMESPACE,
    };
    let has_namespace = table_has_column(&source, "meta", "namespace")?;
    if has_namespace {
        let mut stmt = source.prepare("SELECT namespace, key, value FROM meta")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (row_namespace, key, value) = row?;
            target.execute(
                "INSERT OR IGNORE INTO meta(namespace, key, value) VALUES(?1, ?2, ?3)",
                rusqlite::params![row_namespace, key, value],
            )?;
        }
    } else {
        let mut stmt = source.prepare("SELECT key, value FROM meta")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            target.execute(
                "INSERT OR IGNORE INTO meta(namespace, key, value) VALUES(?1, ?2, ?3)",
                rusqlite::params![namespace, key, value],
            )?;
        }
    }
    Ok(())
}

fn is_typed_todo_id(id: &str) -> bool {
    let mut parts = id.split('_');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(suffix) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    prefix.len() == 4
        && prefix.chars().all(|c| c.is_ascii_lowercase())
        && suffix.len() == 16
        && suffix.chars().all(|c| c.is_ascii_alphanumeric())
}

fn typed_todo_type(category: &str, title: &str, old_id: &str) -> &'static str {
    let c = category.to_ascii_lowercase();
    let t = title.to_ascii_lowercase();
    let all = format!("{c} {t} {old_id}").to_ascii_lowercase();
    if c.contains("test") || all.contains("test") {
        "test"
    } else if c.contains("doc") || all.contains("readme") || all.contains("doc") {
        "docs"
    } else if c.contains("bug") || all.contains("fix") || all.contains("bug") {
        "bugs"
    } else if c.contains("sec") || all.contains("security") || all.contains("auth") {
        "secu"
    } else if c.contains("perf") || all.contains("perf") {
        "perf"
    } else if c.contains("infra") || all.contains("infra") || all.contains("deploy") {
        "infr"
    } else if c.contains("backend") || c == "database" || all.contains("server") {
        "bend"
    } else if c.contains("frontend") || all.contains("ui") || all.contains("web") {
        "fend"
    } else if c == "ci" || all.contains("ci") || all.contains("pipeline") {
        "cicd"
    } else if c.contains("refactor") || all.contains("cleanup") {
        "reft"
    } else if c.contains("tool") || all.contains("cli") {
        "tool"
    } else if c.contains("feature") || all.contains("feature") || all.contains("implement") {
        "feat"
    } else {
        "arch"
    }
}

fn typed_todo_suffix(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
        if out.len() >= 16 {
            out.truncate(16);
            break;
        }
    }
    out
}

fn rewrite_csv_task_ids(csv: &str, id_map: &HashMap<String, String>) -> String {
    let mut changed = false;
    let mut mapped = Vec::new();
    for part in csv.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(new_id) = id_map.get(token) {
            changed = true;
            mapped.push(new_id.clone());
        } else {
            mapped.push(token.to_string());
        }
    }
    if changed {
        mapped.join(",")
    } else {
        csv.to_string()
    }
}

fn rewrite_json_task_ids(value: &mut Value, id_map: &HashMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(mapped) = id_map.get(s) {
                *s = mapped.clone();
            } else if s.contains(',') {
                *s = rewrite_csv_task_ids(s, id_map);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_json_task_ids(item, id_map);
            }
        }
        Value::Object(obj) => {
            for v in obj.values_mut() {
                rewrite_json_task_ids(v, id_map);
            }
        }
        _ => {}
    }
}

fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, error::DecapodError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(error::DecapodError::RusqliteError)?;
    let mut rows = stmt.query([]).map_err(error::DecapodError::RusqliteError)?;
    while let Some(row) = rows.next().map_err(error::DecapodError::RusqliteError)? {
        let name: String = row.get(1).map_err(error::DecapodError::RusqliteError)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, error::DecapodError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(true),
    )
    .optional()
    .map_err(error::DecapodError::RusqliteError)
    .map(|v| v.unwrap_or(false))
}

fn table_columns(
    conn: &Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, error::DecapodError> {
    let pragma = format!("PRAGMA {schema}.table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

fn migrate_todo_ids_to_typed_format(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    let todo_db = data_root.join(schemas::LOCAL_DB_NAME);
    if !todo_db.exists() {
        return Ok(());
    }

    let mut conn = db::db_connect(&todo_db.to_string_lossy())?;
    let tasks_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false);
    if !tasks_exists {
        return Ok(());
    }

    let mut existing_ids = HashSet::new();
    let mut legacy_rows = Vec::new();
    {
        let has_category = table_has_column(&conn, "tasks", "category")?;
        let has_title = table_has_column(&conn, "tasks", "title")?;
        let select_sql = match (has_category, has_title) {
            (true, true) => "SELECT id, category, title FROM tasks ORDER BY created_at, id",
            (true, false) => "SELECT id, category, '' as title FROM tasks ORDER BY created_at, id",
            (false, true) => "SELECT id, '' as category, title FROM tasks ORDER BY created_at, id",
            (false, false) => {
                "SELECT id, '' as category, '' as title FROM tasks ORDER BY created_at, id"
            }
        };
        let mut stmt = conn
            .prepare(select_sql)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                ))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        for row in rows {
            let (id, category, title) = row.map_err(error::DecapodError::RusqliteError)?;
            existing_ids.insert(id.clone());
            if !is_typed_todo_id(&id) {
                legacy_rows.push((id, category, title));
            }
        }
    }
    if legacy_rows.is_empty() {
        return Ok(());
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    for (old_id, category, title) in legacy_rows {
        let task_type = typed_todo_type(&category, &title, &old_id);
        let mut attempt = 0usize;
        loop {
            let seed = if attempt == 0 {
                old_id.clone()
            } else {
                format!("{old_id}:{attempt}")
            };
            let candidate = format!("{}_{}", task_type, typed_todo_suffix(&seed));
            if candidate == old_id {
                id_map.insert(old_id.clone(), candidate);
                break;
            }
            if !existing_ids.contains(&candidate) && !id_map.values().any(|v| v == &candidate) {
                id_map.insert(old_id.clone(), candidate.clone());
                existing_ids.insert(candidate);
                break;
            }
            attempt += 1;
        }
    }

    let sql = include_str!("sql/todo_task_id_v15_migration.sql");
    conn.execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(error::DecapodError::RusqliteError)?;
    let tx = conn
        .transaction()
        .map_err(error::DecapodError::RusqliteError)?;

    tx.execute(
        "CREATE TEMP TABLE task_id_migration_map(
            old_id TEXT PRIMARY KEY,
            new_id TEXT NOT NULL UNIQUE
        )",
        [],
    )
    .map_err(error::DecapodError::RusqliteError)?;
    for (old_id, new_id) in &id_map {
        tx.execute(
            "INSERT INTO task_id_migration_map(old_id, new_id) VALUES(?1, ?2)",
            [old_id, new_id],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    let full_schema_compatible = table_has_column(&tx, "tasks", "parent_task_id")?
        && table_exists(&tx, "task_verification")?
        && table_has_column(&tx, "task_verification", "todo_id")?
        && table_exists(&tx, "task_owners")?
        && table_has_column(&tx, "task_owners", "task_id")?
        && table_exists(&tx, "task_dependencies")?
        && table_has_column(&tx, "task_dependencies", "task_id")?
        && table_has_column(&tx, "task_dependencies", "depends_on_task_id")?
        && table_exists(&tx, "task_events")?
        && table_has_column(&tx, "task_events", "task_id")?;

    if full_schema_compatible {
        tx.execute_batch(sql)
            .map_err(error::DecapodError::RusqliteError)?;
    } else {
        let run_if = |cond: bool, statement: &str| -> Result<(), error::DecapodError> {
            if cond {
                tx.execute(statement, [])
                    .map_err(error::DecapodError::RusqliteError)?;
            }
            Ok(())
        };
        run_if(
            table_has_column(&tx, "tasks", "parent_task_id")?,
            "UPDATE tasks
             SET parent_task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = tasks.parent_task_id
             )
             WHERE parent_task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_verification")?
                && table_has_column(&tx, "task_verification", "todo_id")?,
            "UPDATE task_verification
             SET todo_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_verification.todo_id
             )
             WHERE todo_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_owners")? && table_has_column(&tx, "task_owners", "task_id")?,
            "UPDATE task_owners
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_owners.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_dependencies")?
                && table_has_column(&tx, "task_dependencies", "task_id")?,
            "UPDATE task_dependencies
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_dependencies.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_dependencies")?
                && table_has_column(&tx, "task_dependencies", "depends_on_task_id")?,
            "UPDATE task_dependencies
             SET depends_on_task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_dependencies.depends_on_task_id
             )
             WHERE depends_on_task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        run_if(
            table_exists(&tx, "task_events")? && table_has_column(&tx, "task_events", "task_id")?,
            "UPDATE task_events
             SET task_id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = task_events.task_id
             )
             WHERE task_id IN (SELECT old_id FROM task_id_migration_map)",
        )?;
        tx.execute(
            "UPDATE tasks
             SET id = (
                 SELECT m.new_id FROM task_id_migration_map m WHERE m.old_id = tasks.id
             )
             WHERE id IN (SELECT old_id FROM task_id_migration_map)",
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    {
        let has_depends_on = table_has_column(&tx, "tasks", "depends_on")?;
        let has_blocks = table_has_column(&tx, "tasks", "blocks")?;
        let select_sql = match (has_depends_on, has_blocks) {
            (true, true) => "SELECT id, depends_on, blocks FROM tasks",
            (true, false) => "SELECT id, depends_on, '' as blocks FROM tasks",
            (false, true) => "SELECT id, '' as depends_on, blocks FROM tasks",
            (false, false) => "SELECT id, '' as depends_on, '' as blocks FROM tasks",
        };
        let mut stmt = tx
            .prepare(select_sql)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                ))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        let mut rewrites = Vec::new();
        for row in rows {
            let (task_id, depends_on, blocks) = row.map_err(error::DecapodError::RusqliteError)?;
            let next_depends = rewrite_csv_task_ids(&depends_on, &id_map);
            let next_blocks = rewrite_csv_task_ids(&blocks, &id_map);
            if next_depends != depends_on || next_blocks != blocks {
                rewrites.push((task_id, next_depends, next_blocks));
            }
        }
        drop(stmt);
        if has_depends_on || has_blocks {
            for (task_id, depends_on, blocks) in rewrites {
                match (has_depends_on, has_blocks) {
                    (true, true) => {
                        tx.execute(
                            "UPDATE tasks SET depends_on = ?1, blocks = ?2 WHERE id = ?3",
                            rusqlite::params![depends_on, blocks, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (true, false) => {
                        tx.execute(
                            "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
                            rusqlite::params![depends_on, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (false, true) => {
                        tx.execute(
                            "UPDATE tasks SET blocks = ?1 WHERE id = ?2",
                            rusqlite::params![blocks, task_id],
                        )
                        .map_err(error::DecapodError::RusqliteError)?;
                    }
                    (false, false) => {}
                }
            }
        }
    }

    if tx
        .query_row(
            "SELECT 1 FROM pragma_table_info('tasks') WHERE name='hash'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false)
    {
        tx.execute(
            "UPDATE tasks
             SET hash = lower(substr(id, instr(id, '_') + 1, 6))
             WHERE instr(id, '_') > 0",
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    if table_exists(&tx, "task_events")? && table_has_column(&tx, "task_events", "payload")? {
        let mut stmt = tx
            .prepare("SELECT event_id, payload FROM task_events")
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        let mut payload_rewrites = Vec::new();
        for row in rows {
            let (event_id, payload_raw) = row.map_err(error::DecapodError::RusqliteError)?;
            if let Ok(mut payload_json) = serde_json::from_str::<Value>(&payload_raw) {
                rewrite_json_task_ids(&mut payload_json, &id_map);
                if let Ok(next_raw) = serde_json::to_string(&payload_json)
                    && next_raw != payload_raw
                {
                    payload_rewrites.push((event_id, next_raw));
                }
            }
        }
        drop(stmt);
        for (event_id, payload) in payload_rewrites {
            tx.execute(
                "UPDATE task_events SET payload = ?1 WHERE event_id = ?2",
                rusqlite::params![payload, event_id],
            )
            .map_err(error::DecapodError::RusqliteError)?;
        }
    }

    tx.commit().map_err(error::DecapodError::RusqliteError)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(error::DecapodError::RusqliteError)?;

    // Legacy JSONL is an immutable migration input. The consolidated event
    // table is the rewrite target; never rewrite the archive in place.
    if table_exists(&conn, "todo_events")? {
        let mut stmt = conn
            .prepare("SELECT event_id, payload FROM todo_events")
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error::DecapodError::RusqliteError)?;
        let mut updates = Vec::new();
        for row in rows {
            let (event_id, raw) = row.map_err(error::DecapodError::RusqliteError)?;
            if let Ok(mut value) = serde_json::from_str::<Value>(&raw) {
                rewrite_json_task_ids(&mut value, &id_map);
                let next = serde_json::to_string(&value)
                    .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
                if next != raw {
                    updates.push((event_id, next));
                }
            }
        }
        drop(stmt);
        for (event_id, payload) in updates {
            conn.execute(
                "UPDATE todo_events SET payload = ?1 WHERE event_id = ?2",
                rusqlite::params![payload, event_id],
            )?;
        }
    }

    Ok(())
}

fn migrate_todo_one_shot_column(decapod_root: &Path) -> Result<(), error::DecapodError> {
    let data_root = decapod_root.join("data");
    let todo_db = data_root.join(schemas::LOCAL_DB_NAME);
    if !todo_db.exists() {
        return Ok(());
    }

    let conn = db::db_connect(&todo_db.to_string_lossy())?;
    let tasks_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false);
    if !tasks_exists {
        return Ok(());
    }

    let has_one_shot = table_has_column(&conn, "tasks", "one_shot")?;
    if !has_one_shot {
        conn.execute(
            "ALTER TABLE tasks ADD COLUMN one_shot INTEGER DEFAULT 0",
            [],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    Ok(())
}

fn migrate_table(
    data_root: &Path,
    source_db: &str,
    target_conn: &Connection,
    table: &str,
) -> Result<(), error::DecapodError> {
    let source_path = data_root.join(source_db);
    if !source_path.exists() {
        return Ok(());
    }

    target_conn
        .execute(
            "ATTACH DATABASE ?1 AS source",
            [source_path.to_string_lossy().as_ref()],
        )
        .map_err(error::DecapodError::RusqliteError)?;

    let source_has_table = target_conn
        .query_row(
            "SELECT 1 FROM source.sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?
        .unwrap_or(false);
    let res = if source_has_table {
        let source_columns = table_columns(target_conn, "source", table)?;
        let target_columns = table_columns(target_conn, "main", table)?;
        let columns: Vec<String> = target_columns
            .into_iter()
            .filter(|column| source_columns.iter().any(|source| source == column))
            .collect();
        if columns.is_empty() {
            Ok(0)
        } else {
            let quoted = columns
                .iter()
                .map(|column| format!("\"{column}\""))
                .collect::<Vec<_>>()
                .join(", ");
            target_conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO main.{table} ({quoted}) SELECT {quoted} FROM source.{table}"
                ),
                [],
            )
        }
    } else {
        Ok(0)
    };

    target_conn
        .execute("DETACH DATABASE source", [])
        .map_err(error::DecapodError::RusqliteError)?;

    res.map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/migration_tests.rs"]
mod tests;
