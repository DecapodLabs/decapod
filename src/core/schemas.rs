//! Centralized database schema definitions for all Decapod consolidated bins.
//!
//! Decapod uses 4 consolidated SQLite databases ("bins") to manage state:
//! 1. governance.db: Rules, policies, health, feedback, and archives.
//! 2. memory.db: Governed knowledge graph, decisions, and aptitude preferences.
//! 3. automation.db: Scheduled tasks (cron) and event triggers (reflex).
//! 4. todo.db: Transactional task tracking with event-sourcing.

// --- 1. Governance Bin ---
pub const GOVERNANCE_DB_NAME: &str = "governance.db";

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

pub const FEEDBACK_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS feedback (
        id TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        text TEXT NOT NULL,
        links TEXT,
        created_at TEXT NOT NULL
    )
";

pub const ARCHIVE_DB_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS archives (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        summary_hash TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
";

pub const GOVERNANCE_DB_SCHEMA_OBLIGATIONS: &str = "
    CREATE TABLE IF NOT EXISTS obligations (
        id TEXT PRIMARY KEY,
        intent_ref TEXT NOT NULL,
        risk_tier TEXT NOT NULL,
        required_proofs TEXT NOT NULL, -- JSON array of claim IDs or proof labels
        state_commit_root TEXT,
        status TEXT NOT NULL DEFAULT 'open', -- open, met, failed
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        metadata TEXT -- JSON blob for extra info
    )
";

pub const GOVERNANCE_DB_SCHEMA_OBLIGATION_EDGES: &str = "
    CREATE TABLE IF NOT EXISTS obligation_edges (
        edge_id TEXT PRIMARY KEY,
        from_id TEXT NOT NULL,
        to_id TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'depends_on',
        created_at TEXT NOT NULL,
        UNIQUE(from_id, to_id),
        FOREIGN KEY(from_id) REFERENCES obligations(id) ON DELETE CASCADE,
        FOREIGN KEY(to_id) REFERENCES obligations(id) ON DELETE CASCADE
    )
";

// --- 2. Memory Bin ---
pub const MEMORY_DB_NAME: &str = "memory.db";
pub const MEMORY_EVENTS_NAME: &str = "memory.events.jsonl";
pub const MEMORY_SCHEMA_VERSION: u32 = 1;

pub const MEMORY_DB_SCHEMA_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
";

pub const MEMORY_DB_SCHEMA_NODES: &str = "
    CREATE TABLE IF NOT EXISTS nodes (
        id TEXT PRIMARY KEY,
        node_type TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        priority TEXT NOT NULL DEFAULT 'notable',
        confidence TEXT NOT NULL DEFAULT 'agent_inferred',
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        scope TEXT NOT NULL DEFAULT 'repo',
        tags TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        effective_from TEXT,
        effective_to TEXT,
        dir_path TEXT NOT NULL,
        actor TEXT NOT NULL DEFAULT 'decapod'
    )
";

pub const MEMORY_DB_SCHEMA_SOURCES: &str = "
    CREATE TABLE IF NOT EXISTS sources (
        id TEXT PRIMARY KEY,
        node_id TEXT NOT NULL,
        source TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY(node_id) REFERENCES nodes(id)
    )
";

pub const MEMORY_DB_SCHEMA_EDGES: &str = "
    CREATE TABLE IF NOT EXISTS edges (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        target_id TEXT NOT NULL,
        edge_type TEXT NOT NULL,
        created_at TEXT NOT NULL,
        actor TEXT NOT NULL DEFAULT 'decapod',
        FOREIGN KEY(source_id) REFERENCES nodes(id),
        FOREIGN KEY(target_id) REFERENCES nodes(id)
    )
";

pub const MEMORY_DB_SCHEMA_EVENTS: &str = "
    CREATE TABLE IF NOT EXISTS federation_events (
        event_id TEXT PRIMARY KEY,
        ts TEXT NOT NULL,
        event_type TEXT NOT NULL,
        node_id TEXT,
        payload TEXT NOT NULL,
        actor TEXT NOT NULL
    )
";

pub const MEMORY_DB_INDEX_NODES_TYPE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_nodes_type ON nodes(node_type)";
pub const MEMORY_DB_INDEX_NODES_STATUS: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_nodes_status ON nodes(status)";
pub const MEMORY_DB_INDEX_NODES_SCOPE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_nodes_scope ON nodes(scope)";
pub const MEMORY_DB_INDEX_NODES_PRIORITY: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_nodes_priority ON nodes(priority)";
pub const MEMORY_DB_INDEX_NODES_UPDATED: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_nodes_updated ON nodes(updated_at)";
pub const MEMORY_DB_INDEX_SOURCES_NODE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_sources_node ON sources(node_id)";
pub const MEMORY_DB_INDEX_EDGES_SOURCE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_edges_source ON edges(source_id)";
pub const MEMORY_DB_INDEX_EDGES_TARGET: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_edges_target ON edges(target_id)";
pub const MEMORY_DB_INDEX_EDGES_TYPE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_edges_type ON edges(edge_type)";
pub const MEMORY_DB_INDEX_EVENTS_NODE: &str =
    "CREATE INDEX IF NOT EXISTS idx_fed_events_node ON federation_events(node_id)";

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
        scope TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        merge_key TEXT DEFAULT '',
        supersedes_id TEXT,
        ttl_policy TEXT NOT NULL DEFAULT 'persistent',
        expires_ts TEXT
    )
";

pub const KNOWLEDGE_DB_INDEX_STATUS: &str =
    "CREATE INDEX IF NOT EXISTS idx_knowledge_status ON knowledge(status)";
pub const KNOWLEDGE_DB_INDEX_CREATED: &str =
    "CREATE INDEX IF NOT EXISTS idx_knowledge_created ON knowledge(created_at)";
pub const KNOWLEDGE_DB_INDEX_MERGE_KEY: &str =
    "CREATE INDEX IF NOT EXISTS idx_knowledge_merge_key ON knowledge(merge_key)";
pub const KNOWLEDGE_DB_INDEX_ACTIVE_MERGE_SCOPE: &str = "CREATE INDEX IF NOT EXISTS idx_knowledge_active_merge_scope ON knowledge(status, merge_key, scope)";

// Legacy Decide Schemas (preserved for migration)
pub const DECIDE_DB_SCHEMA_SESSIONS: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY,
        tree_id TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT DEFAULT '',
        status TEXT NOT NULL DEFAULT 'active',
        federation_node_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        dir_path TEXT NOT NULL,
        scope TEXT NOT NULL DEFAULT 'repo',
        actor TEXT NOT NULL DEFAULT 'decapod'
    )
";
pub const DECIDE_DB_SCHEMA_DECISIONS: &str = "
    CREATE TABLE IF NOT EXISTS decisions (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        question_id TEXT NOT NULL,
        tree_id TEXT NOT NULL,
        question_text TEXT NOT NULL,
        chosen_value TEXT NOT NULL,
        chosen_label TEXT NOT NULL,
        rationale TEXT DEFAULT '',
        user_note TEXT DEFAULT '',
        federation_node_id TEXT,
        created_at TEXT NOT NULL,
        actor TEXT NOT NULL DEFAULT 'decapod',
        FOREIGN KEY(session_id) REFERENCES sessions(id)
    )
";

// Decoupled name mapping for compatibility
pub const KNOWLEDGE_DB_NAME: &str = "knowledge.db";
pub const FEDERATION_DB_NAME: &str = "federation.db";
pub const FEDERATION_EVENTS_NAME: &str = "federation.events.jsonl";
pub const FEDERATION_SCHEMA_VERSION: u32 = 1;
pub const FEDERATION_DB_SCHEMA_META: &str = MEMORY_DB_SCHEMA_META;
pub const FEDERATION_DB_SCHEMA_NODES: &str = MEMORY_DB_SCHEMA_NODES;
pub const FEDERATION_DB_SCHEMA_SOURCES: &str = MEMORY_DB_SCHEMA_SOURCES;
pub const FEDERATION_DB_SCHEMA_EDGES: &str = MEMORY_DB_SCHEMA_EDGES;
pub const FEDERATION_DB_SCHEMA_EVENTS: &str = MEMORY_DB_SCHEMA_EVENTS;
pub const FEDERATION_DB_INDEX_NODES_TYPE: &str = MEMORY_DB_INDEX_NODES_TYPE;
pub const FEDERATION_DB_INDEX_NODES_STATUS: &str = MEMORY_DB_INDEX_NODES_STATUS;
pub const FEDERATION_DB_INDEX_NODES_SCOPE: &str = MEMORY_DB_INDEX_NODES_SCOPE;
pub const FEDERATION_DB_INDEX_NODES_PRIORITY: &str = MEMORY_DB_INDEX_NODES_PRIORITY;
pub const FEDERATION_DB_INDEX_NODES_UPDATED: &str = MEMORY_DB_INDEX_NODES_UPDATED;
pub const FEDERATION_DB_INDEX_SOURCES_NODE: &str = MEMORY_DB_INDEX_SOURCES_NODE;
pub const FEDERATION_DB_INDEX_EDGES_SOURCE: &str = MEMORY_DB_INDEX_EDGES_SOURCE;
pub const FEDERATION_DB_INDEX_EDGES_TARGET: &str = MEMORY_DB_INDEX_EDGES_TARGET;
pub const FEDERATION_DB_INDEX_EDGES_TYPE: &str = MEMORY_DB_INDEX_EDGES_TYPE;
pub const FEDERATION_DB_INDEX_EVENTS_NODE: &str = MEMORY_DB_INDEX_EVENTS_NODE;

pub const DECIDE_DB_NAME: &str = "decisions.db";
pub const DECIDE_SCHEMA_VERSION: u32 = 1;
pub const DECIDE_DB_SCHEMA_META: &str = MEMORY_DB_SCHEMA_META;
pub const DECIDE_DB_INDEX_DECISIONS_SESSION: &str =
    "CREATE INDEX IF NOT EXISTS idx_decisions_session ON decisions(session_id)";
pub const DECIDE_DB_INDEX_DECISIONS_TREE: &str =
    "CREATE INDEX IF NOT EXISTS idx_decisions_tree ON decisions(tree_id)";
pub const DECIDE_DB_INDEX_SESSIONS_TREE: &str =
    "CREATE INDEX IF NOT EXISTS idx_sessions_tree ON sessions(tree_id)";
pub const DECIDE_DB_INDEX_SESSIONS_STATUS: &str =
    "CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status)";

// --- 3. Automation Bin ---
pub const AUTOMATION_DB_NAME: &str = "automation.db";
pub const CRON_DB_NAME: &str = "cron.db";
pub const REFLEX_DB_NAME: &str = "reflex.db";

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

// --- 4. Transactional Bin (TODO) ---
pub const TODO_DB_NAME: &str = "todo.db";
pub const TODO_EVENTS_NAME: &str = "todo.events.jsonl";
pub const TODO_SCHEMA_VERSION: u32 = 14;

pub const TODO_DB_SCHEMA_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_TASKS: &str = "
    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        hash TEXT NOT NULL,
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
        component TEXT DEFAULT '',
        assigned_to TEXT DEFAULT '',
        assigned_at TEXT
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
pub const TODO_DB_SCHEMA_INDEX_HASH: &str =
    "CREATE INDEX IF NOT EXISTS idx_tasks_hash ON tasks(hash)";
pub const TODO_DB_SCHEMA_INDEX_EVENTS_TASK: &str =
    "CREATE INDEX IF NOT EXISTS idx_events_task ON task_events(task_id)";

pub const TODO_DB_SCHEMA_TASK_VERIFICATION: &str = "
    CREATE TABLE IF NOT EXISTS task_verification (
        todo_id TEXT PRIMARY KEY,
        proof_plan TEXT NOT NULL DEFAULT '[]',
        verification_artifacts TEXT,
        last_verified_at TEXT,
        last_verified_status TEXT,
        last_verified_notes TEXT,
        verification_policy_days INTEGER NOT NULL DEFAULT 90,
        updated_at TEXT NOT NULL,
        FOREIGN KEY(todo_id) REFERENCES tasks(id) ON DELETE CASCADE
    )
";

pub const TODO_DB_SCHEMA_INDEX_VERIFICATION_STATUS: &str = "
    CREATE INDEX IF NOT EXISTS idx_task_verification_status
    ON task_verification(last_verified_status)
";

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

pub const TODO_DB_SCHEMA_AGENT_CATEGORY_CLAIMS: &str = "
    CREATE TABLE IF NOT EXISTS agent_category_claims (
        id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        category TEXT NOT NULL UNIQUE,
        claimed_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_INDEX_AGENT_CATEGORY_AGENT: &str =
    "CREATE INDEX IF NOT EXISTS idx_agent_category_agent ON agent_category_claims(agent_id)";

pub const TODO_DB_SCHEMA_AGENT_PRESENCE: &str = "
    CREATE TABLE IF NOT EXISTS agent_presence (
        agent_id TEXT PRIMARY KEY,
        last_seen TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        updated_at TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_INDEX_AGENT_PRESENCE_LAST_SEEN: &str =
    "CREATE INDEX IF NOT EXISTS idx_agent_presence_last_seen ON agent_presence(last_seen)";

pub const TODO_DB_SCHEMA_AGENT_TRUST: &str = "
    CREATE TABLE IF NOT EXISTS agent_trust (
        agent_id TEXT PRIMARY KEY,
        trust_level TEXT NOT NULL DEFAULT 'basic',
        granted_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        granted_by TEXT NOT NULL DEFAULT 'system'
    )
";

pub const TODO_DB_SCHEMA_INDEX_AGENT_TRUST_LEVEL: &str =
    "CREATE INDEX IF NOT EXISTS idx_agent_trust_level ON agent_trust(trust_level)";

pub const TODO_DB_SCHEMA_RISK_ZONES: &str = "
    CREATE TABLE IF NOT EXISTS risk_zones (
        id TEXT PRIMARY KEY,
        zone_name TEXT NOT NULL UNIQUE,
        description TEXT DEFAULT '',
        required_trust_level TEXT NOT NULL DEFAULT 'basic',
        requires_approval BOOLEAN NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    )
";

pub const TODO_DB_SCHEMA_INDEX_RISK_ZONES_NAME: &str =
    "CREATE INDEX IF NOT EXISTS idx_risk_zones_name ON risk_zones(zone_name)";

pub const TODO_DB_SCHEMA_TASK_OWNERS: &str = "
    CREATE TABLE IF NOT EXISTS task_owners (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL,
        agent_id TEXT NOT NULL,
        claimed_at TEXT NOT NULL,
        claim_type TEXT NOT NULL DEFAULT 'primary',
        FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
    )
";

pub const TODO_DB_SCHEMA_INDEX_TASK_OWNERS_TASK: &str =
    "CREATE INDEX IF NOT EXISTS idx_task_owners_task ON task_owners(task_id)";

pub const TODO_DB_SCHEMA_TASK_DEPENDENCIES: &str = "
    CREATE TABLE IF NOT EXISTS task_dependencies (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL,
        depends_on_task_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(task_id, depends_on_task_id),
        FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE,
        FOREIGN KEY(depends_on_task_id) REFERENCES tasks(id) ON DELETE CASCADE
    )
";

pub const TODO_DB_SCHEMA_INDEX_TASK_DEPS_TASK: &str =
    "CREATE INDEX IF NOT EXISTS idx_task_dependencies_task ON task_dependencies(task_id)";
pub const TODO_DB_SCHEMA_INDEX_TASK_DEPS_DEPENDS_ON: &str = "CREATE INDEX IF NOT EXISTS idx_task_dependencies_depends_on ON task_dependencies(depends_on_task_id)";

pub const TODO_DB_SCHEMA_AGENT_EXPERTISE: &str = "
    CREATE TABLE IF NOT EXISTS agent_expertise (
        id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        category TEXT NOT NULL,
        expertise_level TEXT NOT NULL DEFAULT 'intermediate',
        claimed_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(agent_id, category)
    )
";

pub const TODO_DB_SCHEMA_INDEX_AGENT_EXPERTISE_AGENT: &str =
    "CREATE INDEX IF NOT EXISTS idx_agent_expertise_agent ON agent_expertise(agent_id)";

pub const HEALTH_DB_NAME: &str = "health.db";
pub const POLICY_DB_NAME: &str = "policy.db";
pub const FEEDBACK_DB_NAME: &str = "feedback.db";
pub const ARCHIVE_DB_NAME: &str = "archive.db";
pub const APTITUDE_DB_NAME: &str = "aptitude.db";

// --- 5. LCM Bin (Lossless Context Management) ---
pub const LCM_DB_NAME: &str = "lcm.db";
pub const LCM_EVENTS_NAME: &str = "lcm.events.jsonl";

pub const LCM_DB_SCHEMA_ORIGINALS_INDEX: &str = "
    CREATE TABLE IF NOT EXISTS originals_index (
        content_hash TEXT PRIMARY KEY,
        event_id TEXT NOT NULL,
        ts TEXT NOT NULL,
        actor TEXT NOT NULL,
        kind TEXT NOT NULL,
        byte_size INTEGER NOT NULL,
        session_id TEXT
    )
";

pub const LCM_DB_SCHEMA_SUMMARIES: &str = "
    CREATE TABLE IF NOT EXISTS summaries (
        summary_hash TEXT PRIMARY KEY,
        ts TEXT NOT NULL,
        scope TEXT NOT NULL,
        original_hashes TEXT NOT NULL,
        summary_text TEXT NOT NULL,
        token_estimate INTEGER NOT NULL
    )
";

pub const LCM_DB_SCHEMA_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
";

pub const LCM_DB_INDEX_ORIGINALS_KIND: &str =
    "CREATE INDEX IF NOT EXISTS idx_lcm_originals_kind ON originals_index(kind)";
pub const LCM_DB_INDEX_ORIGINALS_TS: &str =
    "CREATE INDEX IF NOT EXISTS idx_lcm_originals_ts ON originals_index(ts)";
pub const LCM_DB_INDEX_SUMMARIES_SCOPE: &str =
    "CREATE INDEX IF NOT EXISTS idx_lcm_summaries_scope ON summaries(scope)";

// --- 6. Map Operators ---
pub const MAP_EVENTS_NAME: &str = "map.events.jsonl";
