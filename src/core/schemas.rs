// src/schemas.rs
// Centralized database schema definitions for Decapod subsystems.

// --- Knowledge ---
pub const KNOWLEDGE_DB_NAME: &str = "knowledge.db";
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
pub const TODO_DB_NAME: &str = "todo.db";
pub const TODO_EVENTS_NAME: &str = "todo.events.jsonl";
pub const TODO_SCHEMA_VERSION: u32 = 1;

pub const TODO_DB_SCHEMA_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_TASKS: &str = "
    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
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
        blocks TEXT DEFAULT ''
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
