//! Centralized database schema definitions for all Decapod subsystems.
//!
//! This module contains the canonical SQL schemas for every Decapod database.
//! Schemas are versioned and deterministic - changes require migration events.
//!
//! # For AI Agents
//!
//! - **Do not modify schemas directly**: Propose changes via `decapod feedback propose`
//! - **Schemas are binding contracts**: Subsystems depend on these exact structures
//! - **Schema versioning**: Some subsystems track schema versions in `meta` tables
//! - **Deterministic replay**: Event-sourced subsystems (TODO, health) rebuild from these schemas

// --- Knowledge ---

/// Knowledge database file name
pub const KNOWLEDGE_DB_NAME: &str = "knowledge.db";

/// Knowledge database schema SQL
///
/// Stores structured knowledge entries with provenance tracking.
/// Each entry must reference its source (claim, proof, or external pointer).
pub const KNOWLEDGE_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS knowledge (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        provenance TEXT NOT NULL,
        claim_id TEXT,
        tags TEXT DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT,
        dir_path TEXT NOT NULL,
        scope TEXT NOT NULL
    )
";

// --- TODO ---

/// TODO database file name
pub const TODO_DB_NAME: &str = "todo.db";

/// TODO event log file name
///
/// Event-sourced append-only log for deterministic rebuild of todo.db
pub const TODO_EVENTS_NAME: &str = "todo.events.jsonl";

/// TODO database schema version
///
/// Used for migration tracking in the `meta` table
pub const TODO_SCHEMA_VERSION: u32 = 5;

/// TODO metadata table schema
///
/// Stores schema version and other metadata key-value pairs
pub const TODO_DB_SCHEMA_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
";

/// TODO tasks table schema
///
/// Core task tracking table with full lifecycle support (open → in-progress → completed → closed)
pub const TODO_DB_SCHEMA_TASKS: &str = "
    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        description TEXT DEFAULT '',
        tags TEXT DEFAULT '',
        owner TEXT DEFAULT '',
        due TEXT,
        ref TEXT DEFAULT '',
        status TEXT NOT NULL DEFAULT 'open',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        closed_at TEXT,
        dir_path TEXT NOT NULL,
        scope TEXT NOT NULL,
        parent_task_id TEXT,
        priority TEXT DEFAULT 'medium',
        depends_on TEXT DEFAULT '',
        blocks TEXT DEFAULT '',
        category TEXT DEFAULT '',
        component TEXT DEFAULT ''
    )
";

pub const TODO_DB_SCHEMA_TASK_EVENTS: &str = "
    CREATE TABLE IF NOT EXISTS task_events (
        event_id TEXT PRIMARY KEY,
        ts TEXT NOT NULL,
        event_type TEXT NOT NULL,
        task_id TEXT,
        payload TEXT NOT NULL,
        actor TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_INDEX_STATUS: &str =
    "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)";
pub const TODO_DB_SCHEMA_INDEX_SCOPE: &str =
    "CREATE INDEX IF NOT EXISTS idx_tasks_scope ON tasks(scope)";
pub const TODO_DB_SCHEMA_INDEX_DIR: &str =
    "CREATE INDEX IF NOT EXISTS idx_tasks_dir ON tasks(dir_path)";
pub const TODO_DB_SCHEMA_INDEX_EVENTS_TASK: &str =
    "CREATE INDEX IF NOT EXISTS idx_events_task ON task_events(task_id)";

/// Task categories table schema
///
/// Predefined categories that agents can claim ownership of:
/// - Software lifecycle: features, bugs, docs, ci, refactor, tests, security, performance
/// - By subsystem: backend, frontend, api, database, infra, tooling, ux
pub const TODO_DB_SCHEMA_CATEGORIES: &str = "
    CREATE TABLE IF NOT EXISTS categories (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        description TEXT DEFAULT '',
        keywords TEXT DEFAULT '',
        created_at TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_INDEX_CATEGORY_NAME: &str =
    "CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name)";

// --- Cron ---
pub const CRON_DB_NAME: &str = "cron.db";
pub const CRON_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS cron_jobs (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT DEFAULT '',
        schedule TEXT NOT NULL,
        command TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        last_run TEXT,
        next_run TEXT,
        tags TEXT DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT,
        dir_path TEXT NOT NULL,
        scope TEXT NOT NULL
    )
";

// --- Reflex ---
pub const REFLEX_DB_NAME: &str = "reflex.db";
pub const REFLEX_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS reflexes (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT DEFAULT '',
        trigger_type TEXT NOT NULL,
        trigger_config TEXT DEFAULT '{}',
        action_type TEXT NOT NULL,
        action_config TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        tags TEXT DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT,
        dir_path TEXT NOT NULL,
        scope TEXT NOT NULL
    )
";

// --- Health ---
pub const HEALTH_DB_NAME: &str = "health.db";
pub const HEALTH_DB_SCHEMA_CLAIMS: &str = "
    CREATE TABLE IF NOT EXISTS claims (
        id TEXT PRIMARY KEY,
        subject TEXT NOT NULL,
        kind TEXT NOT NULL,
        provenance TEXT,
        created_at TEXT NOT NULL
    )
";
pub const HEALTH_DB_SCHEMA_PROOF_EVENTS: &str = "
    CREATE TABLE IF NOT EXISTS proof_events (
        event_id TEXT PRIMARY KEY,
        claim_id TEXT NOT NULL,
        ts TEXT NOT NULL,
        surface TEXT NOT NULL,
        result TEXT NOT NULL,
        sla_seconds INTEGER NOT NULL,
        FOREIGN KEY(claim_id) REFERENCES claims(id)
    )
";
pub const HEALTH_DB_SCHEMA_HEALTH_CACHE: &str = "
    CREATE TABLE IF NOT EXISTS health_cache (
        claim_id TEXT PRIMARY KEY,
        computed_state TEXT NOT NULL,
        reason TEXT,
        updated_at TEXT NOT NULL,
        FOREIGN KEY(claim_id) REFERENCES claims(id)
    )
";

// --- Policy ---
pub const POLICY_DB_NAME: &str = "policy.db";
pub const POLICY_DB_SCHEMA_APPROVALS: &str = "
    CREATE TABLE IF NOT EXISTS approvals (
        approval_id TEXT PRIMARY KEY,
        action_fingerprint TEXT NOT NULL,
        actor TEXT NOT NULL,
        ts TEXT NOT NULL,
        scope TEXT NOT NULL,
        expires_at TEXT
    )
";
pub const POLICY_DB_SCHEMA_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_approvals_fingerprint ON approvals(action_fingerprint)";

// --- Archive ---
pub const ARCHIVE_DB_NAME: &str = "archive.db";
pub const ARCHIVE_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS archives (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        summary_hash TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
";

// --- Feedback ---
pub const FEEDBACK_DB_NAME: &str = "feedback.db";
pub const FEEDBACK_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS feedback (
        id TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        text TEXT NOT NULL,
        links TEXT,
        created_at TEXT NOT NULL
    )
";
