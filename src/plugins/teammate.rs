//! Teammate plugin: remembers user preferences, skills, and behaviors.
//!
//! This plugin catalogs distinct user expectations like:
//! - SSH key preferences for Git operations
//! - Branch naming conventions
//! - Code style preferences
//! - Commit message formats
//! - Workflow conventions
//! - Learned skills and workflows
//! - Pattern recognition for auto-detection
//!
//! # For AI Agents
//!
//! - **Check preferences before acting**: Use `decapod teammate get <key>` to check user preferences
//! - **Record new preferences**: When user expresses a preference, record it with `decapod teammate add`
//! - **Observe patterns**: Use `decapod teammate observe` to capture observations for pattern matching
//! - **Get contextual prompts**: Use `decapod teammate prompt` to get relevant reminders for your context
//! - **Categories organize preferences**: Use categories like "git", "style", "workflow" for organization

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::store::Store;
use regex::Regex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const TEAMMATE_DB_NAME: &str = "teammate.db";

// ============================================================================
// DATABASE SCHEMAS
// ============================================================================

pub const TEAMMATE_DB_SCHEMA_PREFERENCES: &str = "
    CREATE TABLE IF NOT EXISTS preferences (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        context TEXT,
        source TEXT NOT NULL,
        confidence INTEGER DEFAULT 100,
        created_at TEXT NOT NULL,
        updated_at TEXT,
        last_accessed_at TEXT,
        access_count INTEGER DEFAULT 0,
        UNIQUE(category, key)
    )
";

pub const TEAMMATE_DB_SCHEMA_SKILLS: &str = "
    CREATE TABLE IF NOT EXISTS skills (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        description TEXT,
        workflow TEXT NOT NULL,
        context TEXT,
        usage_count INTEGER DEFAULT 0,
        last_used_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT
    )
";

pub const TEAMMATE_DB_SCHEMA_PATTERNS: &str = "
    CREATE TABLE IF NOT EXISTS patterns (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        category TEXT NOT NULL,
        regex_pattern TEXT NOT NULL,
        preference_category TEXT,
        preference_key TEXT,
        description TEXT,
        created_at TEXT NOT NULL
    )
";

pub const TEAMMATE_DB_SCHEMA_OBSERVATIONS: &str = "
    CREATE TABLE IF NOT EXISTS observations (
        id TEXT PRIMARY KEY,
        content TEXT NOT NULL,
        category TEXT,
        matched_pattern_id TEXT,
        processed INTEGER DEFAULT 0,
        created_at TEXT NOT NULL,
        FOREIGN KEY(matched_pattern_id) REFERENCES patterns(id)
    )
";

pub const TEAMMATE_DB_SCHEMA_CONSOLIDATIONS: &str = "
    CREATE TABLE IF NOT EXISTS consolidations (
        id TEXT PRIMARY KEY,
        source_type TEXT NOT NULL,
        source_id TEXT NOT NULL,
        target_type TEXT NOT NULL,
        target_id TEXT NOT NULL,
        reason TEXT,
        created_at TEXT NOT NULL
    )
";

pub const TEAMMATE_DB_SCHEMA_AGENT_PROMPTS: &str = "
    CREATE TABLE IF NOT EXISTS agent_prompts (
        id TEXT PRIMARY KEY,
        context TEXT NOT NULL,
        prompt_text TEXT NOT NULL,
        priority INTEGER DEFAULT 100,
        active INTEGER DEFAULT 1,
        usage_count INTEGER DEFAULT 0,
        last_shown_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT
    )
";

// Index creation statements
pub const TEAMMATE_DB_SCHEMA_INDEX_PREF_CATEGORY: &str =
    "CREATE INDEX IF NOT EXISTS idx_preferences_category ON preferences(category)";
pub const TEAMMATE_DB_SCHEMA_INDEX_PREF_KEY: &str =
    "CREATE INDEX IF NOT EXISTS idx_preferences_key ON preferences(key)";
pub const TEAMMATE_DB_SCHEMA_INDEX_PREF_ACCESS: &str =
    "CREATE INDEX IF NOT EXISTS idx_preferences_access ON preferences(last_accessed_at)";
pub const TEAMMATE_DB_SCHEMA_INDEX_SKILL_NAME: &str =
    "CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name)";
pub const TEAMMATE_DB_SCHEMA_INDEX_PATTERN_CATEGORY: &str =
    "CREATE INDEX IF NOT EXISTS idx_patterns_category ON patterns(category)";
pub const TEAMMATE_DB_SCHEMA_INDEX_OBS_PROCESSED: &str =
    "CREATE INDEX IF NOT EXISTS idx_observations_processed ON observations(processed)";
pub const TEAMMATE_DB_SCHEMA_INDEX_PROMPT_CONTEXT: &str =
    "CREATE INDEX IF NOT EXISTS idx_agent_prompts_context ON agent_prompts(context)";

// ============================================================================
// DEFAULT DATA
// ============================================================================

#[allow(clippy::type_complexity)]
const DEFAULT_PATTERNS: &[(&str, &str, &str, Option<&str>, Option<&str>, &str)] = &[
    (
        "ssh_preference",
        "preferences",
        r"(?i)(?:use|prefer)\s+(?:ssh\s+)?key\s+(\w+)",
        Some("git"),
        Some("ssh_key"),
        "Detects SSH key preferences",
    ),
    (
        "commit_style_conventional",
        "preferences",
        r"(?i)(?:use|follow)\s+conventional\s+commits?",
        Some("git"),
        Some("commit_style"),
        "Detects conventional commit preference",
    ),
    (
        "branch_naming",
        "preferences",
        r"(?i)(?:branch\s+name|naming)\s+(?:with|using)\s+(\w+[/-]\w+)",
        Some("git"),
        Some("branch_pattern"),
        "Detects branch naming conventions",
    ),
    (
        "always_statement",
        "preferences",
        r"(?i)always\s+(\w+(?:\s+\w+){0,5})",
        None,
        None,
        "Detects 'always' preference statements",
    ),
    (
        "never_statement",
        "preferences",
        r"(?i)never\s+(\w+(?:\s+\w+){0,5})",
        None,
        None,
        "Detects 'never' preference statements",
    ),
    (
        "prefer_statement",
        "preferences",
        r"(?i)prefer\s+(?:to\s+)?(\w+(?:\s+\w+){0,10})",
        None,
        None,
        "Detects 'prefer' preference statements",
    ),
];

const DEFAULT_AGENT_PROMPTS: &[(&str, &str, i64)] = &[
    (
        "git_operations",
        "Check teammate preferences for: SSH key usage, branch naming conventions, commit message style",
        100,
    ),
    (
        "code_style",
        "Check teammate preferences for: formatting rules, naming conventions, style preferences",
        90,
    ),
    (
        "workflow",
        "Check teammate preferences for: testing requirements, documentation needs, review processes",
        80,
    ),
    (
        "preference_recording",
        "When user expresses a preference (always/never/prefer), use 'decapod teammate add' to record it",
        95,
    ),
];

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preference {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub context: Option<String>,
    pub source: String,
    pub confidence: i64,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub last_accessed_at: Option<String>,
    pub access_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreferenceInput {
    pub category: String,
    pub key: String,
    pub value: String,
    pub context: Option<String>,
    pub source: String,
    pub confidence: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub workflow: String,
    pub context: Option<String>,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillInput {
    pub name: String,
    pub description: Option<String>,
    pub workflow: String,
    pub context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub category: String,
    pub regex_pattern: String,
    pub preference_category: Option<String>,
    pub preference_key: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternInput {
    pub name: String,
    pub category: String,
    pub regex_pattern: String,
    pub preference_category: Option<String>,
    pub preference_key: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Observation {
    pub id: String,
    pub content: String,
    pub category: Option<String>,
    pub matched_pattern_id: Option<String>,
    pub processed: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Consolidation {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    pub target_type: String,
    pub target_id: String,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentPrompt {
    pub id: String,
    pub context: String,
    pub prompt_text: String,
    pub priority: i64,
    pub active: bool,
    pub usage_count: i64,
    pub last_shown_at: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimilarityGroup {
    pub category: String,
    pub key: String,
    pub preferences: Vec<Preference>,
    pub similarity_reason: String,
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

pub fn teammate_db_path(root: &Path) -> PathBuf {
    root.join(crate::core::schemas::MEMORY_DB_NAME)
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

// ============================================================================
// DATABASE INITIALIZATION
// ============================================================================

pub fn initialize_teammate_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = teammate_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "teammate.init", |conn| {
        // Create tables (if not exists)
        conn.execute(TEAMMATE_DB_SCHEMA_PREFERENCES, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_SKILLS, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_PATTERNS, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_OBSERVATIONS, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_CONSOLIDATIONS, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_AGENT_PROMPTS, [])?;

        // Schema migrations: add columns if they don't exist
        // These will fail silently if columns already exist
        let _ = conn.execute("ALTER TABLE preferences ADD COLUMN confidence INTEGER DEFAULT 100", []);
        let _ = conn.execute("ALTER TABLE preferences ADD COLUMN last_accessed_at TEXT", []);
        let _ = conn.execute("ALTER TABLE preferences ADD COLUMN access_count INTEGER DEFAULT 0", []);

        // Create indexes
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_PREF_CATEGORY, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_PREF_KEY, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_PREF_ACCESS, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_SKILL_NAME, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_PATTERN_CATEGORY, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_OBS_PROCESSED, [])?;
        conn.execute(TEAMMATE_DB_SCHEMA_INDEX_PROMPT_CONTEXT, [])?;

        // Insert default patterns
        let now = now_iso();
        for (name, category, pattern, pref_cat, pref_key, desc) in DEFAULT_PATTERNS {
            conn.execute(
                "INSERT OR IGNORE INTO patterns(id, name, category, regex_pattern, preference_category, preference_key, description, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    ulid::Ulid::new().to_string(),
                    name,
                    category,
                    pattern,
                    pref_cat,
                    pref_key,
                    desc,
                    now
                ],
            )?;
        }

        // Insert default agent prompts
        for (context, prompt, priority) in DEFAULT_AGENT_PROMPTS {
            conn.execute(
                "INSERT OR IGNORE INTO agent_prompts(id, context, prompt_text, priority, active, usage_count, created_at)
                 VALUES(?1, ?2, ?3, ?4, 1, 0, ?5)",
                params![ulid::Ulid::new().to_string(), context, prompt, priority, now],
            )?;
        }

        Ok(())
    })
}

// ============================================================================
// PREFERENCE CRUD
// ============================================================================

pub fn add_preference(
    store: &Store,
    input: PreferenceInput,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();
    let confidence = input.confidence.unwrap_or(100);

    broker.with_conn(&db_path, "decapod", None, "teammate.add", |conn| {
        conn.execute(
            "INSERT INTO preferences(id, category, key, value, context, source, confidence, created_at, updated_at, last_accessed_at, access_count)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, 0)
             ON CONFLICT(category, key) DO UPDATE SET
                value = excluded.value,
                context = excluded.context,
                source = excluded.source,
                confidence = excluded.confidence,
                updated_at = ?8",
            params![
                id,
                input.category,
                input.key,
                input.value,
                input.context,
                input.source,
                confidence,
                now
            ],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn get_preference(
    store: &Store,
    category: &str,
    key: &str,
) -> Result<Option<Preference>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let now = now_iso();

    let pref = broker.with_conn(&db_path, "decapod", None, "teammate.get", |conn| {
        // First, update access metrics
        conn.execute(
            "UPDATE preferences SET access_count = access_count + 1, last_accessed_at = ?1
             WHERE category = ?2 AND key = ?3",
            params![now, category, key],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, category, key, value, context, source, confidence, created_at, updated_at, last_accessed_at, access_count
             FROM preferences WHERE category = ?1 AND key = ?2",
        )?;
        let result = stmt.query_row(params![category, key], |row| {
            Ok(Preference {
                id: row.get(0)?,
                category: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                context: row.get(4)?,
                source: row.get(5)?,
                confidence: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                last_accessed_at: row.get(9)?,
                access_count: row.get(10)?,
            })
        });

        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(error::DecapodError::RusqliteError(e)),
        }
    })?;

    Ok(pref)
}

pub fn get_preference_by_id(
    store: &Store,
    id: &str,
) -> Result<Option<Preference>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let pref = broker.with_conn(&db_path, "decapod", None, "teammate.get_by_id", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, category, key, value, context, source, confidence, created_at, updated_at, last_accessed_at, access_count
             FROM preferences WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(Preference {
                id: row.get(0)?,
                category: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                context: row.get(4)?,
                source: row.get(5)?,
                confidence: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                last_accessed_at: row.get(9)?,
                access_count: row.get(10)?,
            })
        });

        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(error::DecapodError::RusqliteError(e)),
        }
    })?;

    Ok(pref)
}

fn row_to_preference(row: &rusqlite::Row) -> Result<Preference, rusqlite::Error> {
    Ok(Preference {
        id: row.get(0)?,
        category: row.get(1)?,
        key: row.get(2)?,
        value: row.get(3)?,
        context: row.get(4)?,
        source: row.get(5)?,
        confidence: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        last_accessed_at: row.get(9)?,
        access_count: row.get(10)?,
    })
}

pub fn list_preferences(
    store: &Store,
    category: Option<&str>,
) -> Result<Vec<Preference>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let entries = broker.with_conn(&db_path, "decapod", None, "teammate.list", |conn| {
        let mut out = Vec::new();

        if let Some(cat) = category {
            let mut stmt = conn.prepare(
                "SELECT id, category, key, value, context, source, confidence, created_at, updated_at, last_accessed_at, access_count
                 FROM preferences WHERE category = ?1 ORDER BY key",
            )?;
            let rows = stmt.query_map([cat], row_to_preference)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, category, key, value, context, source, confidence, created_at, updated_at, last_accessed_at, access_count
                 FROM preferences ORDER BY category, key",
            )?;
            let rows = stmt.query_map([], row_to_preference)?;
            for r in rows {
                out.push(r?);
            }
        }

        Ok(out)
    })?;

    Ok(entries)
}

pub fn delete_preference(
    store: &Store,
    category: &str,
    key: &str,
) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let deleted = broker.with_conn(&db_path, "decapod", None, "teammate.delete", |conn| {
        let rows = conn.execute(
            "DELETE FROM preferences WHERE category = ?1 AND key = ?2",
            params![category, key],
        )?;
        Ok(rows > 0)
    })?;

    Ok(deleted)
}

pub fn get_preferences_by_category(
    store: &Store,
) -> Result<HashMap<String, Vec<Preference>>, error::DecapodError> {
    let all = list_preferences(store, None)?;
    let mut grouped: HashMap<String, Vec<Preference>> = HashMap::new();

    for pref in all {
        grouped.entry(pref.category.clone()).or_default().push(pref);
    }

    Ok(grouped)
}

// ============================================================================
// SKILL CRUD
// ============================================================================

pub fn add_skill(store: &Store, input: SkillInput) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "teammate.skill.add", |conn| {
        conn.execute(
            "INSERT INTO skills(id, name, description, workflow, context, usage_count, last_used_at, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, NULL)
             ON CONFLICT(name) DO UPDATE SET
                description = excluded.description,
                workflow = excluded.workflow,
                context = excluded.context,
                updated_at = ?6",
            params![id, input.name, input.description, input.workflow, input.context, now],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn get_skill(store: &Store, name: &str) -> Result<Option<Skill>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let now = now_iso();

    let skill = broker.with_conn(&db_path, "decapod", None, "teammate.skill.get", |conn| {
        // Update usage metrics
        conn.execute(
            "UPDATE skills SET usage_count = usage_count + 1, last_used_at = ?1 WHERE name = ?2",
            params![now, name],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, name, description, workflow, context, usage_count, last_used_at, created_at, updated_at
             FROM skills WHERE name = ?1",
        )?;
        let result = stmt.query_row(params![name], |row| {
            Ok(Skill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                workflow: row.get(3)?,
                context: row.get(4)?,
                usage_count: row.get(5)?,
                last_used_at: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        });

        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(error::DecapodError::RusqliteError(e)),
        }
    })?;

    Ok(skill)
}

pub fn list_skills(store: &Store) -> Result<Vec<Skill>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let skills = broker.with_conn(&db_path, "decapod", None, "teammate.skill.list", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, workflow, context, usage_count, last_used_at, created_at, updated_at
             FROM skills ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Skill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                workflow: row.get(3)?,
                context: row.get(4)?,
                usage_count: row.get(5)?,
                last_used_at: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    Ok(skills)
}

pub fn delete_skill(store: &Store, name: &str) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let deleted = broker.with_conn(&db_path, "decapod", None, "teammate.skill.delete", |conn| {
        let rows = conn.execute("DELETE FROM skills WHERE name = ?1", params![name])?;
        Ok(rows > 0)
    })?;

    Ok(deleted)
}

// ============================================================================
// PATTERN MANAGEMENT
// ============================================================================

pub fn add_pattern(store: &Store, input: PatternInput) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();

    // Validate regex pattern
    if Regex::new(&input.regex_pattern).is_err() {
        return Err(error::DecapodError::ValidationError(
            "Invalid regex pattern".into(),
        ));
    }

    broker.with_conn(&db_path, "decapod", None, "teammate.pattern.add", |conn| {
        conn.execute(
            "INSERT INTO patterns(id, name, category, regex_pattern, preference_category, preference_key, description, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(name) DO UPDATE SET
                category = excluded.category,
                regex_pattern = excluded.regex_pattern,
                preference_category = excluded.preference_category,
                preference_key = excluded.preference_key,
                description = excluded.description",
            params![
                id,
                input.name,
                input.category,
                input.regex_pattern,
                input.preference_category,
                input.preference_key,
                input.description,
                now
            ],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn list_patterns(store: &Store) -> Result<Vec<Pattern>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let patterns = broker.with_conn(&db_path, "decapod", None, "teammate.pattern.list", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, category, regex_pattern, preference_category, preference_key, description, created_at
             FROM patterns ORDER BY category, name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Pattern {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                regex_pattern: row.get(3)?,
                preference_category: row.get(4)?,
                preference_key: row.get(5)?,
                description: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    Ok(patterns)
}

pub fn match_patterns(
    store: &Store,
    content: &str,
) -> Result<Vec<(Pattern, Vec<String>)>, error::DecapodError> {
    let patterns = list_patterns(store)?;
    let mut matches = Vec::new();

    for pattern in patterns {
        if let Ok(regex) = Regex::new(&pattern.regex_pattern) {
            let captures: Vec<String> = regex
                .captures_iter(content)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();
            if !captures.is_empty() {
                matches.push((pattern, captures));
            }
        }
    }

    Ok(matches)
}

// ============================================================================
// OBSERVATION MANAGEMENT
// ============================================================================

pub fn record_observation(
    store: &Store,
    content: &str,
    category: Option<&str>,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();

    // Try to match against patterns
    let patterns = match_patterns(store, content)?;
    let matched_pattern_id = patterns.first().map(|(p, _)| p.id.clone());

    broker.with_conn(&db_path, "decapod", None, "teammate.observe", |conn| {
        conn.execute(
            "INSERT INTO observations(id, content, category, matched_pattern_id, processed, created_at)
             VALUES(?1, ?2, ?3, ?4, 0, ?5)",
            params![id, content, category, matched_pattern_id, now],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn list_pending_observations(
    store: &Store,
    limit: Option<usize>,
) -> Result<Vec<Observation>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let observations = broker.with_conn(&db_path, "decapod", None, "teammate.pending", |conn| {
        let query = format!(
            "SELECT id, content, category, matched_pattern_id, processed, created_at
             FROM observations WHERE processed = 0 ORDER BY created_at DESC LIMIT {}",
            limit.unwrap_or(100)
        );
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(Observation {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                matched_pattern_id: row.get(3)?,
                processed: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    Ok(observations)
}

pub fn mark_observation_processed(store: &Store, id: &str) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let updated = broker.with_conn(
        &db_path,
        "decapod",
        None,
        "teammate.observe.process",
        |conn| {
            let rows = conn.execute(
                "UPDATE observations SET processed = 1 WHERE id = ?1",
                params![id],
            )?;
            Ok(rows > 0)
        },
    )?;

    Ok(updated)
}

// ============================================================================
// CONSOLIDATION MANAGEMENT
// ============================================================================

pub fn record_consolidation(
    store: &Store,
    source_type: &str,
    source_id: &str,
    target_type: &str,
    target_id: &str,
    reason: Option<&str>,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "teammate.consolidate.record", |conn| {
        conn.execute(
            "INSERT INTO consolidations(id, source_type, source_id, target_type, target_id, reason, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, source_type, source_id, target_type, target_id, reason, now],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn analyze_similarity(store: &Store) -> Result<Vec<SimilarityGroup>, error::DecapodError> {
    let preferences = list_preferences(store, None)?;
    let mut groups: HashMap<(String, String), Vec<Preference>> = HashMap::new();

    // Group by category and key prefix (first 3 chars)
    for pref in preferences {
        let key_prefix = if pref.key.len() >= 3 {
            pref.key[..3].to_string()
        } else {
            pref.key.clone()
        };
        groups
            .entry((pref.category.clone(), key_prefix))
            .or_default()
            .push(pref);
    }

    let mut similarity_groups = Vec::new();
    for ((category, key_prefix), prefs) in groups {
        if prefs.len() > 1 {
            similarity_groups.push(SimilarityGroup {
                category: category.clone(),
                key: format!("{}*", key_prefix),
                preferences: prefs,
                similarity_reason: format!(
                    "Multiple preferences with similar keys in category '{}'",
                    category
                ),
            });
        }
    }

    Ok(similarity_groups)
}

pub fn execute_consolidation(
    store: &Store,
    group: &SimilarityGroup,
    target_id: &str,
) -> Result<bool, error::DecapodError> {
    // Mark all preferences in the group as consolidated into the target
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "teammate.consolidate.execute", |conn| {
        for pref in &group.preferences {
            if pref.id != target_id {
                conn.execute(
                    "INSERT INTO consolidations(id, source_type, source_id, target_type, target_id, reason, created_at)
                     VALUES(?1, 'preference', ?2, 'preference', ?3, ?4, ?5)",
                    params![
                        ulid::Ulid::new().to_string(),
                        pref.id,
                        target_id,
                        format!("Consolidated: {}", group.similarity_reason),
                        now_iso()
                    ],
                )?;
            }
        }
        Ok(())
    })?;

    Ok(true)
}

// ============================================================================
// AGENT PROMPT MANAGEMENT
// ============================================================================

pub fn add_agent_prompt(
    store: &Store,
    context: &str,
    prompt_text: &str,
    priority: Option<i64>,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let id = ulid::Ulid::new().to_string();
    let now = now_iso();
    let priority = priority.unwrap_or(100);

    broker.with_conn(&db_path, "decapod", None, "teammate.prompt.add", |conn| {
        conn.execute(
            "INSERT INTO agent_prompts(id, context, prompt_text, priority, active, usage_count, created_at)
             VALUES(?1, ?2, ?3, ?4, 1, 0, ?5)",
            params![id, context, prompt_text, priority, now],
        )?;
        Ok(())
    })?;

    Ok(id)
}

pub fn get_prompts_for_context(
    store: &Store,
    context: &str,
    limit: Option<usize>,
) -> Result<Vec<AgentPrompt>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);
    let now = now_iso();

    let prompts = broker.with_conn(&db_path, "decapod", None, "teammate.prompt.get", |conn| {
        let query = format!(
            "SELECT id, context, prompt_text, priority, active, usage_count, last_shown_at, created_at, updated_at
             FROM agent_prompts 
             WHERE active = 1 AND (context = ?1 OR context = 'global')
             ORDER BY priority DESC, usage_count ASC
             LIMIT {}",
            limit.unwrap_or(5)
        );
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params![context], |row| {
            Ok(AgentPrompt {
                id: row.get(0)?,
                context: row.get(1)?,
                prompt_text: row.get(2)?,
                priority: row.get(3)?,
                active: row.get::<_, i64>(4)? != 0,
                usage_count: row.get(5)?,
                last_shown_at: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    // Update usage metrics
    for prompt in &prompts {
        broker.with_conn(&db_path, "decapod", None, "teammate.prompt.update_usage", |conn| {
            conn.execute(
                "UPDATE agent_prompts SET usage_count = usage_count + 1, last_shown_at = ?1 WHERE id = ?2",
                params![now, prompt.id],
            )?;
            Ok(())
        })?;
    }

    Ok(prompts)
}

pub fn list_agent_prompts(store: &Store) -> Result<Vec<AgentPrompt>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = teammate_db_path(&store.root);

    let prompts = broker.with_conn(&db_path, "decapod", None, "teammate.prompt.list", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, context, prompt_text, priority, active, usage_count, last_shown_at, created_at, updated_at
             FROM agent_prompts ORDER BY context, priority DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AgentPrompt {
                id: row.get(0)?,
                context: row.get(1)?,
                prompt_text: row.get(2)?,
                priority: row.get(3)?,
                active: row.get::<_, i64>(4)? != 0,
                usage_count: row.get(5)?,
                last_shown_at: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    Ok(prompts)
}

pub fn generate_contextual_reminders(
    store: &Store,
    context: &str,
) -> Result<Vec<String>, error::DecapodError> {
    let mut reminders = Vec::new();

    // Get relevant prompts
    let prompts = get_prompts_for_context(store, context, Some(3))?;
    for prompt in prompts {
        reminders.push(prompt.prompt_text);
    }

    // Get relevant preferences
    let prefs = list_preferences(store, Some(context))?;
    for pref in prefs.iter().take(3) {
        reminders.push(format!(
            "Preference [{}.{}]: {} (confidence: {}%)",
            pref.category, pref.key, pref.value, pref.confidence
        ));
    }

    // Get relevant skills
    let skills = list_skills(store)?;
    for skill in skills.iter().take(2) {
        reminders.push(format!(
            "Skill [{}]: {} (used {} times)",
            skill.name,
            skill.description.as_deref().unwrap_or("No description"),
            skill.usage_count
        ));
    }

    Ok(reminders)
}

// ============================================================================
// SCHEMA INFO
// ============================================================================

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "teammate",
        "version": "0.2.0",
        "description": "User preference, skill, and behavior memory system with pattern recognition",
        "commands": [
            { "name": "add", "description": "Add or update a preference", "parameters": ["category", "key", "value", "context", "source", "confidence"] },
            { "name": "get", "description": "Get a specific preference", "parameters": ["category", "key"] },
            { "name": "list", "description": "List all preferences", "parameters": ["category?"] },
            { "name": "delete", "description": "Delete a preference", "parameters": ["category", "key"] },
            { "name": "skill add", "description": "Add or update a skill", "parameters": ["name", "description", "workflow", "context"] },
            { "name": "skill get", "description": "Get a skill by name", "parameters": ["name"] },
            { "name": "skill list", "description": "List all skills", "parameters": [] },
            { "name": "skill delete", "description": "Delete a skill", "parameters": ["name"] },
            { "name": "observe", "description": "Record an observation for pattern matching", "parameters": ["content", "category?"] },
            { "name": "pending", "description": "List pending observations", "parameters": ["limit?"] },
            { "name": "consolidate", "description": "Analyze and consolidate similar entries", "parameters": ["--dry-run", "--execute"] },
            { "name": "prompt", "description": "Get contextual prompts for agents", "parameters": ["--context", "--format"] },
            { "name": "remind", "description": "Generate contextual reminders", "parameters": ["--context"] }
        ],
        "storage": ["teammate.db"],
        "categories": [
            "git", "style", "workflow", "communication", "tooling"
        ],
        "features": [
            "access_tracking",
            "confidence_levels",
            "pattern_matching",
            "observations",
            "consolidation",
            "agent_prompts"
        ]
    })
}

// ============================================================================
// CLI TYPES AND HANDLERS
// ============================================================================

#[derive(clap::Args, Debug)]
pub struct TeammateCli {
    #[clap(subcommand)]
    pub command: TeammateCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum TeammateCommand {
    /// Add or update a preference
    Add {
        /// Category (e.g., git, style, workflow)
        #[clap(long)]
        category: String,
        /// Preference key
        #[clap(long)]
        key: String,
        /// Preference value
        #[clap(long)]
        value: String,
        /// Optional context/explanation
        #[clap(long)]
        context: Option<String>,
        /// Source of the preference
        #[clap(long, default_value = "user_request")]
        source: String,
        /// Confidence level (0-100)
        #[clap(long)]
        confidence: Option<i64>,
    },
    /// Get a specific preference
    Get {
        /// Category
        #[clap(long)]
        category: String,
        /// Preference key
        #[clap(long)]
        key: String,
    },
    /// List preferences
    List {
        /// Filter by category
        #[clap(long)]
        category: Option<String>,
        /// Output format
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Delete a preference
    Delete {
        /// Category
        #[clap(long)]
        category: String,
        /// Preference key
        #[clap(long)]
        key: String,
    },
    /// Skill management commands
    #[clap(subcommand)]
    Skill(SkillCommand),
    /// Record an observation
    Observe {
        /// Observation content
        #[clap(long)]
        content: String,
        /// Optional category
        #[clap(long)]
        category: Option<String>,
    },
    /// List pending observations
    Pending {
        /// Maximum number of observations to show
        #[clap(long)]
        limit: Option<usize>,
    },
    /// Analyze and consolidate similar entries
    Consolidate {
        /// Show what would be consolidated without making changes
        #[clap(long)]
        dry_run: bool,
        /// Execute the consolidation
        #[clap(long)]
        execute: bool,
    },
    /// Get contextual prompts for agents
    Prompt {
        /// Context (e.g., git_operations, code_style)
        #[clap(long)]
        context: Option<String>,
        /// Output format (text, json)
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Generate contextual reminders
    Remind {
        /// Context for reminders
        #[clap(long)]
        context: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum SkillCommand {
    /// Add or update a skill
    Add {
        /// Skill name
        #[clap(long)]
        name: String,
        /// Skill description
        #[clap(long)]
        description: Option<String>,
        /// Workflow/steps for the skill
        #[clap(long)]
        workflow: String,
        /// Optional context
        #[clap(long)]
        context: Option<String>,
    },
    /// Get a skill by name
    Get {
        /// Skill name
        #[clap(long)]
        name: String,
    },
    /// List all skills
    List {
        /// Output format
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Delete a skill
    Delete {
        /// Skill name
        #[clap(long)]
        name: String,
    },
}

pub fn run_teammate_cli(store: &Store, cli: TeammateCli) -> Result<(), error::DecapodError> {
    initialize_teammate_db(&store.root)?;

    match cli.command {
        TeammateCommand::Add {
            category,
            key,
            value,
            context,
            source,
            confidence,
        } => {
            let input = PreferenceInput {
                category,
                key: key.clone(),
                value: value.clone(),
                context,
                source,
                confidence,
            };
            let id = add_preference(store, input)?;
            println!("✓ Preference recorded: {}={} (id: {})", key, value, id);
        }
        TeammateCommand::Get { category, key } => match get_preference(store, &category, &key)? {
            Some(pref) => {
                println!("{}: {}", pref.key, pref.value);
                if let Some(ctx) = pref.context {
                    println!("  Context: {}", ctx);
                }
                println!(
                    "  Source: {} | Confidence: {}%",
                    pref.source, pref.confidence
                );
                println!(
                    "  Created: {} | Accessed: {} times",
                    pref.created_at, pref.access_count
                );
                if let Some(last) = pref.last_accessed_at {
                    println!("  Last accessed: {}", last);
                }
            }
            None => {
                println!("No preference found for {}.{}", category, key);
            }
        },
        TeammateCommand::List { category, format } => {
            let prefs = list_preferences(store, category.as_deref())?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&prefs).unwrap());
            } else if prefs.is_empty() {
                println!("No preferences recorded yet.");
            } else {
                let grouped = get_preferences_by_category(store)?;
                for (cat, items) in grouped {
                    println!("\n[{}]", cat);
                    for item in items {
                        println!(
                            "  {} = {} (confidence: {}%, accessed: {}x)",
                            item.key, item.value, item.confidence, item.access_count
                        );
                    }
                }
            }
        }
        TeammateCommand::Delete { category, key } => {
            if delete_preference(store, &category, &key)? {
                println!("✓ Deleted preference {}.{}", category, key);
            } else {
                println!("✗ Preference {}.{} not found", category, key);
            }
        }
        TeammateCommand::Skill(skill_cmd) => match skill_cmd {
            SkillCommand::Add {
                name,
                description,
                workflow,
                context,
            } => {
                let input = SkillInput {
                    name: name.clone(),
                    description,
                    workflow,
                    context,
                };
                let id = add_skill(store, input)?;
                println!("✓ Skill recorded: {} (id: {})", name, id);
            }
            SkillCommand::Get { name } => match get_skill(store, &name)? {
                Some(skill) => {
                    println!("Skill: {}", skill.name);
                    if let Some(desc) = skill.description {
                        println!("  Description: {}", desc);
                    }
                    println!("  Workflow: {}", skill.workflow);
                    if let Some(ctx) = skill.context {
                        println!("  Context: {}", ctx);
                    }
                    println!("  Used: {} times", skill.usage_count);
                    if let Some(last) = skill.last_used_at {
                        println!("  Last used: {}", last);
                    }
                }
                None => {
                    println!("No skill found: {}", name);
                }
            },
            SkillCommand::List { format } => {
                let skills = list_skills(store)?;
                if format == "json" {
                    println!("{}", serde_json::to_string_pretty(&skills).unwrap());
                } else if skills.is_empty() {
                    println!("No skills recorded yet.");
                } else {
                    println!("Skills:");
                    for skill in skills {
                        println!(
                            "  {} - {} (used {}x)",
                            skill.name,
                            skill.description.as_deref().unwrap_or("No description"),
                            skill.usage_count
                        );
                    }
                }
            }
            SkillCommand::Delete { name } => {
                if delete_skill(store, &name)? {
                    println!("✓ Deleted skill {}", name);
                } else {
                    println!("✗ Skill {} not found", name);
                }
            }
        },
        TeammateCommand::Observe { content, category } => {
            let id = record_observation(store, &content, category.as_deref())?;

            // Check for pattern matches
            let matches = match_patterns(store, &content)?;
            if !matches.is_empty() {
                println!("✓ Observation recorded (id: {})", id);
                println!("  Pattern matches found:");
                for (pattern, captures) in matches {
                    println!("    - {}: {:?}", pattern.name, captures);
                    if let (Some(pref_cat), Some(pref_key)) =
                        (&pattern.preference_category, &pattern.preference_key)
                    {
                        println!("      → Suggested preference: {}.{}", pref_cat, pref_key);
                    }
                }
            } else {
                println!("✓ Observation recorded (id: {})", id);
            }
        }
        TeammateCommand::Pending { limit } => {
            let observations = list_pending_observations(store, limit)?;
            if observations.is_empty() {
                println!("No pending observations.");
            } else {
                println!("Pending observations:");
                for obs in observations {
                    println!("  [{}] {}", &obs.id[..8], obs.content);
                    if let Some(cat) = obs.category {
                        println!("      Category: {}", cat);
                    }
                    if let Some(pattern_id) = obs.matched_pattern_id {
                        println!("      Matched pattern: {}", &pattern_id[..8]);
                    }
                }
            }
        }
        TeammateCommand::Consolidate { dry_run, execute } => {
            let groups = analyze_similarity(store)?;

            if groups.is_empty() {
                println!("No similar preferences found for consolidation.");
            } else {
                println!("Found {} groups of similar preferences:", groups.len());
                for (i, group) in groups.iter().enumerate() {
                    println!(
                        "\n  Group {}: {} ({})",
                        i + 1,
                        group.key,
                        group.similarity_reason
                    );
                    for pref in &group.preferences {
                        println!(
                            "    - {}.{} = {} (confidence: {}%, accessed: {}x)",
                            pref.category, pref.key, pref.value, pref.confidence, pref.access_count
                        );
                    }
                }

                if execute && !dry_run {
                    for group in groups {
                        // Use the most accessed preference as target
                        if let Some(target) =
                            group.preferences.iter().max_by_key(|p| p.access_count)
                        {
                            execute_consolidation(store, &group, &target.id)?;
                            println!("\n  Consolidated into: {}.{}", target.category, target.key);
                        }
                    }
                    println!("\n✓ Consolidation complete.");
                } else if dry_run {
                    println!("\n(Dry run - no changes made)");
                } else {
                    println!("\nUse --execute to perform consolidation.");
                }
            }
        }
        TeammateCommand::Prompt { context, format } => {
            let ctx = context.as_deref().unwrap_or("global");
            let prompts = get_prompts_for_context(store, ctx, None)?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&prompts).unwrap());
            } else {
                println!("Prompts for context '{}':", ctx);
                for prompt in prompts {
                    println!(
                        "\n  [{}] (priority: {}, used: {}x)",
                        prompt.context, prompt.priority, prompt.usage_count
                    );
                    println!("  {}", prompt.prompt_text);
                }
            }
        }
        TeammateCommand::Remind { context } => {
            let reminders = generate_contextual_reminders(store, &context)?;

            if reminders.is_empty() {
                println!("No reminders for context '{}'.", context);
            } else {
                println!("Contextual reminders for '{}':", context);
                for (i, reminder) in reminders.iter().enumerate() {
                    println!("\n  {}. {}", i + 1, reminder);
                }
            }
        }
    }

    Ok(())
}
