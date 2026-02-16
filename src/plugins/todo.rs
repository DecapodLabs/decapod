use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::schemas; // Import the new schemas module
use crate::core::store::Store;
use crate::plugins::federation;
use crate::plugins::knowledge;
use crate::plugins::teammate;
use crate::plugins::verify;
use crate::policy;
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::{params, types::ToSql, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use ulid::Ulid;

const AGENT_EVICT_TIMEOUT_SECS: u64 = 30 * 60;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ClaimMode {
    Exclusive,
    Shared,
}

#[derive(Parser, Debug)]
#[clap(name = "todo", about = "Manage TODO tasks within the Decapod system.")]
pub struct TodoCli {
    /// Output format for this command group.
    #[clap(long, global = true, value_enum, default_value = "text")]
    format: OutputFormat,
    #[clap(subcommand)]
    command: TodoCommand,
}

#[derive(Subcommand, Debug)]
pub enum TodoCommand {
    /// Add a new task.
    Add {
        /// Task title (positional argument)
        #[clap(value_name = "TITLE")]
        title: String,
        #[clap(long, default_value = "")]
        description: String,
        #[clap(long, default_value = "medium", value_parser = validate_priority)]
        priority: String,
        #[clap(long, default_value = "")]
        tags: String,
        #[clap(long, default_value = "")]
        owner: String,
        #[clap(long)]
        due: Option<String>,
        #[clap(long, default_value = "")]
        r#ref: String,
        #[clap(long)]
        dir: Option<String>,
        #[clap(long, default_value = "")]
        depends_on: String,
        #[clap(long, default_value = "")]
        blocks: String,
        #[clap(long)]
        parent: Option<String>,
    },
    /// List tasks.
    List {
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        scope: Option<String>,
        #[clap(long)]
        tags: Option<String>,
        #[clap(long)]
        title_search: Option<String>,
        #[clap(long)]
        dir: Option<String>,
    },
    /// Get a task by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// Mark a task done.
    Done {
        #[clap(long)]
        id: String,
        /// Capture verification artifacts and proof baseline while marking done.
        #[clap(long)]
        validated: bool,
        /// File path(s) to hash for drift detection. Defaults to AGENTS.md when --validated is set.
        #[clap(long = "artifact")]
        artifact: Vec<String>,
    },
    /// Archive a task (keeps audit trail).
    Archive {
        #[clap(long)]
        id: String,
    },
    /// Add a comment to a task (audit-only event).
    Comment {
        #[clap(long)]
        id: String,
        #[clap(long)]
        comment: String,
    },
    /// Edit a task's title, description, owner, or category.
    Edit {
        #[clap(long)]
        id: String,
        #[clap(long)]
        title: Option<String>,
        #[clap(long)]
        description: Option<String>,
        #[clap(long)]
        owner: Option<String>,
        #[clap(long)]
        category: Option<String>,
    },
    /// Claim a task for active work (prevents other agents from interfering).
    Claim {
        #[clap(long)]
        id: String,
        /// Agent identifier (defaults to environment or 'unknown').
        #[clap(long)]
        agent: Option<String>,
        /// Claim mode: exclusive takes assignment; shared joins as secondary owner.
        #[clap(long, value_enum, default_value = "exclusive")]
        mode: ClaimMode,
    },
    /// Release a claimed task (makes it available for others).
    Release {
        #[clap(long)]
        id: String,
    },
    /// Rebuild the SQLite DB deterministically from the JSONL event log.
    Rebuild,
    /// List available task categories.
    Categories,
    /// Register an agent and claim ownership of one or more categories.
    RegisterAgent {
        /// Agent identifier (defaults to environment or 'unknown').
        #[clap(long)]
        agent: Option<String>,
        #[clap(long = "category", required = true)]
        categories: Vec<String>,
    },
    /// List current category ownership claims.
    Ownerships {
        /// Filter by category.
        #[clap(long)]
        category: Option<String>,
        /// Filter by agent id.
        #[clap(long)]
        agent: Option<String>,
    },
    /// Record an agent heartbeat.
    Heartbeat {
        /// Agent identifier (defaults to environment or 'unknown').
        #[clap(long)]
        agent: Option<String>,
    },
    /// List agent presence records.
    Presence {
        /// Filter by agent id.
        #[clap(long)]
        agent: Option<String>,
    },
    /// Transfer a task between agents and record handoff artifacts.
    Handoff {
        #[clap(long)]
        id: String,
        #[clap(long)]
        to: String,
        #[clap(long)]
        from: Option<String>,
        #[clap(long)]
        summary: String,
    },
    /// Add an additional owner to a task (supports multiple ownership).
    AddOwner {
        #[clap(long)]
        id: String,
        #[clap(long)]
        agent: String,
        /// Type of ownership claim: primary, secondary, watcher
        #[clap(long, default_value = "secondary")]
        claim_type: String,
    },
    /// Remove an owner from a task.
    RemoveOwner {
        #[clap(long)]
        id: String,
        #[clap(long)]
        agent: String,
    },
    /// List all owners of a task.
    ListOwners {
        #[clap(long)]
        id: String,
    },
    /// Register agent expertise level for a category.
    RegisterExpertise {
        #[clap(long)]
        agent: Option<String>,
        #[clap(long)]
        category: String,
        #[clap(long, default_value = "intermediate")]
        level: String,
    },
    /// List agent expertise claims.
    Expertise {
        #[clap(long)]
        agent: Option<String>,
        #[clap(long)]
        category: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tags: String,
    pub owner: String,
    pub due: Option<String>,
    pub r#ref: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub closed_at: Option<String>,
    pub dir_path: String,
    pub scope: String,
    pub parent_task_id: Option<String>,
    pub priority: String,
    pub depends_on: String,
    pub blocks: String,
    pub category: String,
    pub component: String,
    pub assigned_to: String,
    pub assigned_at: Option<String>,
    #[serde(default)]
    pub owners: Vec<TaskOwner>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskOwner {
    pub agent_id: String,
    pub claim_type: String,
    pub claimed_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TodoEvent {
    ts: String,
    event_id: String,
    event_type: String,
    task_id: Option<String>,
    payload: JsonValue,
    actor: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryOwnership {
    pub id: String,
    pub agent_id: String,
    pub category: String,
    pub claimed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentPresence {
    pub agent_id: String,
    pub last_seen: String,
    pub status: String,
    pub updated_at: String,
}

fn now_iso() -> String {
    // Good enough for stable ordering and human readability; we can switch to chrono later.
    // Use RFC3339-like UTC seconds with 'Z' suffix.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

fn parse_epoch_z(ts: &str) -> Option<u64> {
    ts.trim_end_matches('Z').parse::<u64>().ok()
}

fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize_branch_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '/' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.trim_matches('/').to_string()
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<String, error::DecapodError> {
    let mut current = Some(repo_root);
    let mut store_root = None;
    while let Some(path) = current {
        let candidate = path.join(".decapod").join("data");
        if candidate.exists() {
            store_root = Some(candidate);
            break;
        }
        current = path.parent();
    }
    let store_root = store_root.unwrap_or_else(|| repo_root.join(".decapod").join("data"));

    let capability = match args.first().copied().unwrap_or_default() {
        "status" | "branch" | "rev-parse" => ExternalCapability::VcsRead,
        _ => ExternalCapability::VcsWrite,
    };

    let output = external_action::execute(
        &store_root,
        capability,
        "todo.handoff.reconcile",
        "git",
        args,
        repo_root,
    )?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn reconcile_commit_to_agent_branch(
    repo_root: &Path,
    task_id: &str,
    target_agent: &str,
    summary: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let is_repo = run_git(repo_root, &["rev-parse", "--is-inside-work-tree"]);
    if is_repo.is_err() {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": "not_a_git_repo"
        }));
    }

    let source_branch = run_git(repo_root, &["branch", "--show-current"])?;
    if source_branch.is_empty() {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": "detached_head"
        }));
    }

    let target_branch = format!("{}/work", sanitize_branch_segment(target_agent));
    let status = run_git(repo_root, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": "no_changes",
            "source_branch": source_branch,
            "target_branch": target_branch
        }));
    }

    run_git(repo_root, &["add", "-A"])?;
    let msg = format!("chore(reconcile): handoff {} to {}", task_id, target_agent);
    run_git(repo_root, &["commit", "-m", &msg])?;
    let commit = run_git(repo_root, &["rev-parse", "HEAD"])?;

    if source_branch == target_branch {
        return Ok(serde_json::json!({
            "status": "ok",
            "mode": "same_branch",
            "commit": commit,
            "source_branch": source_branch,
            "target_branch": target_branch
        }));
    }

    let target_exists = run_git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}", target_branch),
        ],
    )
    .is_ok();

    if target_exists {
        run_git(repo_root, &["checkout", &target_branch])?;
        let cherry = run_git(repo_root, &["cherry-pick", &commit]);
        let _ = run_git(repo_root, &["checkout", &source_branch]);
        cherry?;
        Ok(serde_json::json!({
            "status": "ok",
            "mode": "cherry_pick",
            "commit": commit,
            "source_branch": source_branch,
            "target_branch": target_branch,
            "summary": summary
        }))
    } else {
        run_git(repo_root, &["checkout", "-b", &target_branch])?;
        let _ = run_git(repo_root, &["checkout", &source_branch]);
        Ok(serde_json::json!({
            "status": "ok",
            "mode": "created_branch",
            "commit": commit,
            "source_branch": source_branch,
            "target_branch": target_branch,
            "summary": summary
        }))
    }
}

pub fn todo_db_path(root: &Path) -> PathBuf {
    root.join(schemas::TODO_DB_NAME)
}

fn events_path(root: &Path) -> PathBuf {
    root.join(schemas::TODO_EVENTS_NAME)
}

fn connect_todo(root: &Path) -> Result<Connection, error::DecapodError> {
    let db_path = todo_db_path(root);
    crate::db::db_connect(&db_path.to_string_lossy())
}

fn ensure_schema(conn: &Connection) -> Result<(), error::DecapodError> {
    conn.execute(schemas::TODO_DB_SCHEMA_META, [])?;

    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;

    let current_version: u32 = current
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    if current_version >= schemas::TODO_SCHEMA_VERSION {
        return Ok(());
    }

    conn.execute(schemas::TODO_DB_SCHEMA_TASKS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_TASK_EVENTS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_STATUS, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_SCOPE, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_DIR, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_EVENTS_TASK, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_TASK_VERIFICATION, [])?;
    conn.execute(schemas::TODO_DB_SCHEMA_INDEX_VERIFICATION_STATUS, [])?;

    if current_version < 2 {
        conn.execute(schemas::TODO_DB_SCHEMA_CATEGORIES, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_CATEGORY_NAME, [])?;
        seed_default_categories(conn)?;
    }

    if current_version < 3 {
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN category TEXT DEFAULT ''", []);
        migrate_task_categories(conn)?;
    }

    if current_version < 4 {
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN component TEXT DEFAULT ''", []);
        migrate_task_components(conn)?;
    }

    if current_version < 5 {
        let _ = conn.execute(
            "ALTER TABLE tasks ADD COLUMN description TEXT DEFAULT ''",
            [],
        );
    }

    if current_version < 6 {
        conn.execute(schemas::TODO_DB_SCHEMA_TASK_VERIFICATION, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_VERIFICATION_STATUS, [])?;
    }

    if current_version < 7 {
        let _ = conn.execute(
            "ALTER TABLE tasks ADD COLUMN assigned_to TEXT DEFAULT ''",
            [],
        );
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN assigned_at TEXT", []);
    }

    if current_version < 8 {
        conn.execute(schemas::TODO_DB_SCHEMA_AGENT_CATEGORY_CLAIMS, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_AGENT_CATEGORY_AGENT, [])?;
        migrate_existing_category_ownerships(conn)?;
    }

    if current_version < 9 {
        conn.execute(schemas::TODO_DB_SCHEMA_AGENT_PRESENCE, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_AGENT_PRESENCE_LAST_SEEN, [])?;
    }

    if current_version < 10 {
        // Multiple owners support
        conn.execute(schemas::TODO_DB_SCHEMA_TASK_OWNERS, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_TASK_OWNERS_TASK, [])?;
        // Agent expertise support
        conn.execute(schemas::TODO_DB_SCHEMA_AGENT_EXPERTISE, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_AGENT_EXPERTISE_AGENT, [])?;
    }

    if current_version < 11 {
        // Agent trust tiers
        conn.execute(schemas::TODO_DB_SCHEMA_AGENT_TRUST, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_AGENT_TRUST_LEVEL, [])?;
    }

    if current_version < 12 {
        // Operational risk zones
        conn.execute(schemas::TODO_DB_SCHEMA_RISK_ZONES, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_RISK_ZONES_NAME, [])?;
        seed_default_risk_zones(conn)?;
    }

    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [schemas::TODO_SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

fn seed_default_risk_zones(conn: &Connection) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    let zones = vec![
        (
            "todo.claim.exclusive",
            "Exclusive claims require basic trust",
            "basic",
            0,
        ),
        (
            "todo.claim.shared",
            "Shared claims require verified trust",
            "verified",
            0,
        ),
        (
            "todo.handoff",
            "Task handoff requires verified trust and explicit approval",
            "verified",
            1,
        ),
    ];

    for (zone_name, description, required_trust_level, requires_approval) in zones {
        conn.execute(
            "INSERT OR IGNORE INTO risk_zones(id, zone_name, description, required_trust_level, requires_approval, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                Ulid::new().to_string(),
                zone_name,
                description,
                required_trust_level,
                requires_approval,
                ts
            ],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    Ok(())
}

fn seed_default_categories(conn: &Connection) -> Result<(), error::DecapodError> {
    let categories = vec![
        (
            "features",
            "New features and enhancements",
            "feature,add,implement,create new",
        ),
        (
            "bugs",
            "Bug fixes and corrections",
            "fix,bug,issue,error,broken",
        ),
        (
            "docs",
            "Documentation updates",
            "docs,documentation,readme,comment",
        ),
        (
            "ci",
            "CI/CD and automation",
            "ci,github actions,workflow,pipeline,deploy",
        ),
        (
            "refactor",
            "Code refactoring",
            "refactor,restructure,cleanup,improve",
        ),
        (
            "tests",
            "Test coverage and quality",
            "test,spec,coverage,unit,integration",
        ),
        (
            "security",
            "Security improvements",
            "security,auth,permission,vulnerability",
        ),
        (
            "performance",
            "Performance optimizations",
            "perf,performance,speed,optimize",
        ),
        (
            "backend",
            "Backend development",
            "backend,server,api,database",
        ),
        (
            "frontend",
            "Frontend development",
            "frontend,ui,web,css,jsx",
        ),
        ("api", "API design and changes", "api,endpoint,rest,graphql"),
        (
            "database",
            "Database schema and queries",
            "db,database,schema,migration,sql",
        ),
        (
            "infra",
            "Infrastructure and DevOps",
            "infra,docker,kubernetes,cloud",
        ),
        (
            "tooling",
            "Tooling and developer experience",
            "tool,cli,devx,script",
        ),
        ("ux", "User experience and design", "ux,design,ui,usability"),
    ];

    let ts = now_iso();
    for (name, desc, keywords) in categories {
        conn.execute(
            "INSERT OR IGNORE INTO categories(id, name, description, keywords, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![Ulid::new().to_string(), name, desc, keywords, ts],
        )?;
    }
    Ok(())
}

fn migrate_task_categories(conn: &Connection) -> Result<(), error::DecapodError> {
    let mut stmt =
        conn.prepare("SELECT id, title, tags FROM tasks WHERE category = '' OR category IS NULL")?;
    let tasks: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    for (id, title, tags) in tasks {
        if let Some(cat) = infer_category(&title, &tags) {
            conn.execute(
                "UPDATE tasks SET category = ?1 WHERE id = ?2",
                rusqlite::params![cat, id],
            )?;
        }
    }
    Ok(())
}

fn migrate_existing_category_ownerships(conn: &Connection) -> Result<(), error::DecapodError> {
    let mut stmt = conn
        .prepare(
            "SELECT category, assigned_to, MIN(COALESCE(assigned_at, created_at)) AS claimed_at
             FROM tasks
             WHERE category != '' AND assigned_to != '' AND status NOT IN ('done', 'archived')
             GROUP BY category, assigned_to
             ORDER BY claimed_at ASC",
        )
        .map_err(error::DecapodError::RusqliteError)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(error::DecapodError::RusqliteError)?;

    for row in rows {
        let (category, agent_id, claimed_at) = row.map_err(error::DecapodError::RusqliteError)?;
        conn.execute(
            "INSERT OR IGNORE INTO agent_category_claims(id, agent_id, category, claimed_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                Ulid::new().to_string(),
                agent_id,
                category,
                claimed_at.clone(),
                claimed_at
            ],
        )
        .map_err(error::DecapodError::RusqliteError)?;
    }

    Ok(())
}

fn migrate_task_components(conn: &Connection) -> Result<(), error::DecapodError> {
    let mut stmt = conn
        .prepare("SELECT id, title, tags FROM tasks WHERE component = '' OR component IS NULL")?;
    let tasks: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    for (id, title, tags) in tasks {
        if let Some(comp) = infer_component(&title, &tags) {
            conn.execute(
                "UPDATE tasks SET component = ?1 WHERE id = ?2",
                rusqlite::params![comp, id],
            )?;
        }
    }
    Ok(())
}

pub fn infer_component(title: &str, tags: &str) -> Option<String> {
    let text = format!("{} {}", title, tags).to_lowercase();

    let component_keywords = vec![
        ("main", vec!["main", "binary", "cli", "command"]),
        ("lib", vec!["library", "lib", "core", "module"]),
        (
            "actions",
            vec!["github action", "workflow", "ci", "pipeline", "caching"],
        ),
        (
            "teammate",
            vec!["teammate", "preference", "skill", "pattern", "observation"],
        ),
        ("todo", vec!["todo", "task", "work tracking"]),
        ("security", vec!["security", "credential", "key", "auth"]),
        ("intent", vec!["intent", "methodology", "contract"]),
        ("architecture", vec!["architecture", "system design"]),
        ("docs", vec!["docs", "documentation", "readme"]),
        ("templates", vec!["template", "scaffold", "constitution"]),
        ("mise", vec!["mise", "tooling", "version manager"]),
        ("policy", vec!["policy", "risk", "approval"]),
        ("health", vec!["health", "proof", "validate"]),
        ("cron", vec!["cron", "schedule", "automation"]),
        ("reflex", vec!["reflex", "trigger", "hook"]),
    ];

    for (component, keywords) in component_keywords {
        for keyword in keywords {
            if text.contains(keyword) {
                return Some(component.to_string());
            }
        }
    }
    None
}

pub fn infer_category(title: &str, tags: &str) -> Option<String> {
    let text = format!("{} {}", title, tags).to_lowercase();

    let category_keywords = vec![
        (
            "features",
            vec!["feature", "add", "implement", "create new", "new"],
        ),
        (
            "bugs",
            vec!["fix", "bug", "issue", "error", "broken", "repair"],
        ),
        (
            "docs",
            vec!["docs", "documentation", "readme", "comment", "doc"],
        ),
        (
            "ci",
            vec![
                "ci",
                "github action",
                "workflow",
                "pipeline",
                "deploy",
                "github",
            ],
        ),
        (
            "refactor",
            vec!["refactor", "restructure", "cleanup", "improve", "clean"],
        ),
        (
            "tests",
            vec!["test", "spec", "coverage", "unit", "integration", "testing"],
        ),
        (
            "security",
            vec!["security", "auth", "permission", "vulnerability", "secure"],
        ),
        (
            "performance",
            vec!["perf", "performance", "speed", "optimize", "fast"],
        ),
        ("backend", vec!["backend", "server", "database", "db"]),
        (
            "frontend",
            vec!["frontend", "ui", "web", "css", "jsx", "html"],
        ),
        ("api", vec!["api", "endpoint", "rest", "graphql"]),
        (
            "database",
            vec!["database", "db", "schema", "migration", "sql"],
        ),
        (
            "infra",
            vec!["infra", "docker", "kubernetes", "cloud", "aws", "gcp"],
        ),
        ("tooling", vec!["tool", "cli", "devx", "script", "utility"]),
        ("ux", vec!["ux", "design", "usability", "user"]),
    ];

    for (category, keywords) in category_keywords {
        for keyword in keywords {
            if text.contains(keyword) {
                return Some(category.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub created_at: String,
}

pub fn list_categories(root: &Path) -> Result<Vec<Category>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.categories", |conn| {
        ensure_schema(conn)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, keywords, created_at FROM categories ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                keywords: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut categories = Vec::new();
        for r in rows {
            categories.push(r?);
        }
        Ok(categories)
    })
}

fn register_agent_categories(
    root: &Path,
    agent_id: &str,
    categories: &[String],
) -> Result<serde_json::Value, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    let ts = now_iso();

    let normalized: Vec<String> = categories
        .iter()
        .flat_map(|c| c.split(','))
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();

    if normalized.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "At least one non-empty category is required".into(),
        ));
    }

    broker.with_conn(&db_path, "decapod", None, "todo.register_agent", |conn| {
        ensure_schema(conn)?;
        touch_agent_presence(conn, agent_id, &ts)?;

        for category in &normalized {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT name FROM categories WHERE name = ?",
                    [category],
                    |row| row.get(0),
                )
                .optional()
                .map_err(error::DecapodError::RusqliteError)?;

            if exists.is_none() {
                return Err(error::DecapodError::ValidationError(format!(
                    "Unknown category '{}' (run `decapod todo categories`)",
                    category
                )));
            }

            conn.execute(
                "INSERT INTO agent_category_claims(id, agent_id, category, claimed_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(category) DO UPDATE SET
                   agent_id = excluded.agent_id,
                   claimed_at = excluded.claimed_at,
                   updated_at = excluded.updated_at",
                rusqlite::params![Ulid::new().to_string(), agent_id, category, ts, ts],
            )
            .map_err(error::DecapodError::RusqliteError)?;
        }

        Ok(())
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.register_agent",
        "status": "ok",
        "root": root.to_string_lossy(),
        "agent_id": agent_id,
        "categories": normalized,
    }))
}

fn list_category_ownerships(
    root: &Path,
    category: Option<&str>,
    agent: Option<&str>,
) -> Result<Vec<CategoryOwnership>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.ownerships", |conn| {
        ensure_schema(conn)?;
        let mut query = "SELECT id, agent_id, category, claimed_at, updated_at FROM agent_category_claims WHERE 1=1".to_string();
        let mut params: Vec<String> = Vec::new();

        if let Some(c) = category {
            query.push_str(" AND category = ?");
            params.push(c.to_lowercase());
        }
        if let Some(a) = agent {
            query.push_str(" AND agent_id = ?");
            params.push(a.to_string());
        }
        query.push_str(" ORDER BY category");

        let mut stmt = conn
            .prepare(&query)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params.iter().map(|p| p as &dyn ToSql)),
                |row| {
                    Ok(CategoryOwnership {
                        id: row.get(0)?,
                        agent_id: row.get(1)?,
                        category: row.get(2)?,
                        claimed_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(error::DecapodError::RusqliteError)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(error::DecapodError::RusqliteError)?);

        }
        Ok(out)
    })
}

fn touch_agent_presence(
    conn: &Connection,
    agent_id: &str,
    ts: &str,
) -> Result<(), error::DecapodError> {
    conn.execute(
        "INSERT INTO agent_presence(agent_id, last_seen, status, updated_at)
         VALUES(?1, ?2, 'active', ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
           last_seen = excluded.last_seen,
           status = 'active',
           updated_at = excluded.updated_at",
        rusqlite::params![agent_id, ts, ts],
    )
    .map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}

fn is_agent_stale(
    conn: &Connection,
    agent_id: &str,
    now_ts: &str,
    timeout_secs: u64,
) -> Result<bool, error::DecapodError> {
    let last_seen: Option<String> = conn
        .query_row(
            "SELECT last_seen FROM agent_presence WHERE agent_id = ?",
            [agent_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    let Some(last_seen) = last_seen else {
        return Ok(true);
    };

    let Some(now) = parse_epoch_z(now_ts) else {
        return Ok(false);
    };
    let Some(seen) = parse_epoch_z(&last_seen) else {
        return Ok(true);
    };
    Ok(now.saturating_sub(seen) > timeout_secs)
}

fn record_heartbeat(root: &Path, agent_id: &str) -> Result<serde_json::Value, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    let ts = now_iso();
    broker.with_conn(&db_path, "decapod", None, "todo.heartbeat", |conn| {
        ensure_schema(conn)?;
        touch_agent_presence(conn, agent_id, &ts)?;

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "agent.heartbeat".to_string(),
            task_id: None,
            payload: serde_json::json!({ "agent_id": agent_id }),
            actor: agent_id.to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.heartbeat",
        "status": "ok",
        "root": root.to_string_lossy(),
        "agent_id": agent_id,
    }))
}

fn list_agent_presence(
    root: &Path,
    agent: Option<&str>,
) -> Result<Vec<AgentPresence>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "todo.presence", |conn| {
        ensure_schema(conn)?;
        let mut query =
            "SELECT agent_id, last_seen, status, updated_at FROM agent_presence WHERE 1=1"
                .to_string();
        let mut params: Vec<String> = Vec::new();
        if let Some(agent_id) = agent {
            query.push_str(" AND agent_id = ?");
            params.push(agent_id.to_string());
        }
        query.push_str(" ORDER BY last_seen DESC");

        let mut stmt = conn
            .prepare(&query)
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params.iter().map(|p| p as &dyn ToSql)),
                |row| {
                    Ok(AgentPresence {
                        agent_id: row.get(0)?,
                        last_seen: row.get(1)?,
                        status: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(error::DecapodError::RusqliteError)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(error::DecapodError::RusqliteError)?);
        }
        Ok(out)
    })
}

fn get_agent_trust_level(conn: &Connection, agent_id: &str) -> Result<String, error::DecapodError> {
    let level: Option<String> = conn
        .query_row(
            "SELECT trust_level FROM agent_trust WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(level.unwrap_or_else(|| "basic".to_string()))
}

#[allow(dead_code)]
fn set_agent_trust_level(
    conn: &Connection,
    agent_id: &str,
    level: &str,
    granted_by: &str,
) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    conn.execute(
        "INSERT INTO agent_trust(agent_id, trust_level, granted_at, updated_at, granted_by)
         VALUES(?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(agent_id) DO UPDATE SET 
           trust_level=excluded.trust_level, 
           updated_at=excluded.updated_at,
           granted_by=excluded.granted_by",
        rusqlite::params![agent_id, level, ts, ts, granted_by],
    )?;
    Ok(())
}

fn trust_level_to_int(level: &str) -> i32 {
    match level {
        "untrusted" => 0,
        "basic" => 1,
        "verified" => 2,
        "core" => 3,
        _ => 1,
    }
}

fn get_risk_zone_policy(
    conn: &Connection,
    zone_name: &str,
) -> Result<Option<(String, bool)>, error::DecapodError> {
    conn.query_row(
        "SELECT required_trust_level, requires_approval FROM risk_zones WHERE zone_name = ?1",
        rusqlite::params![zone_name],
        |row| {
            let required_trust: String = row.get(0)?;
            let requires_approval: i64 = row.get(1)?;
            Ok((required_trust, requires_approval != 0))
        },
    )
    .optional()
    .map_err(error::DecapodError::RusqliteError)
}

fn enforce_operation_policy(
    root: &Path,
    conn: &Connection,
    zone_name: &str,
    agent_id: &str,
) -> Result<(), error::DecapodError> {
    let Some((required_trust, requires_approval)) = get_risk_zone_policy(conn, zone_name)? else {
        return Ok(());
    };

    let current_level = get_agent_trust_level(conn, agent_id)?;
    if trust_level_to_int(&current_level) < trust_level_to_int(&required_trust) {
        return Err(error::DecapodError::ValidationError(format!(
            "Policy gate denied for {}: agent '{}' trust '{}' < required '{}'",
            zone_name, agent_id, current_level, required_trust
        )));
    }

    if requires_approval {
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root: root.to_path_buf(),
        };
        let level = policy::RiskLevel::HIGH;
        if !policy::human_in_loop_required(&store, zone_name, level, true) {
            return Ok(());
        }
        policy::initialize_policy_db(root)?;
        if !policy::check_approval(&store, zone_name, None, "global")? {
            return Err(error::DecapodError::ValidationError(format!(
                "Policy gate denied for {}: missing approval",
                zone_name
            )));
        }
    }
    Ok(())
}

pub fn check_trust_level(
    root: &Path,
    agent_id: &str,
    required_level: &str,
) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.trust.check", |conn| {
        ensure_schema(conn)?;
        let current_level = get_agent_trust_level(conn, agent_id)?;
        Ok(trust_level_to_int(&current_level) >= trust_level_to_int(required_level))
    })
}

pub fn initialize_todo_db(root: &Path) -> Result<(), error::DecapodError> {
    fs::create_dir_all(root).map_err(error::DecapodError::IoError)?;
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    broker.with_conn(&db_path, "decapod", None, "todo.init", |conn| {
        ensure_schema(conn)?;
        Ok(())
    })?;
    Ok(())
}

fn scope_from_dir(p: &str) -> String {
    const COMPONENT_NAMES: &[&str] = &[
        "application_development",
        "architecture",
        "artificial_intelligence",
        "design_and_style",
        "development_lifecycle",
        "documentation",
        "languages",
        "platform_engineering",
        "project_management",
        "specialized_domains",
    ];

    let path = Path::new(p);
    for component_name in COMPONENT_NAMES {
        if path.file_name().map(|s| s.to_string_lossy().to_lowercase())
            == Some(component_name.to_string())
            || p.to_lowercase().contains(&format!("/{}/", component_name))
        {
            return component_name.to_string();
        }
    }
    "root".to_string()
}

fn task_prefix_from_scope(scope: &str) -> &'static str {
    match scope {
        "application_development" => "AD",
        "architecture" => "AR",
        "artificial_intelligence" => "AI",
        "design_and_style" => "DS",
        "development_lifecycle" => "DL",
        "documentation" => "DO",
        "languages" => "LA",
        "platform_engineering" => "PE",
        "project_management" => "PM",
        "specialized_domains" => "SD",
        _ => "R",
    }
}

fn validate_priority(s: &str) -> Result<String, String> {
    match s {
        "high" | "medium" | "low" => Ok(s.to_string()),
        _ => Err(format!(
            "Invalid priority: {}. Must be one of: high, medium, low",
            s
        )),
    }
}

fn append_event(root: &Path, ev: &TodoEvent) -> Result<(), error::DecapodError> {
    let path = events_path(root);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(ev).unwrap()).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn insert_event(conn: &Connection, ev: &TodoEvent) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO task_events(event_id, ts, event_type, task_id, payload, actor)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            ev.event_id,
            ev.ts,
            ev.event_type,
            ev.task_id,
            serde_json::to_string(&ev.payload).unwrap(),
            ev.actor
        ],
    )?;
    Ok(())
}

pub fn record_task_event(
    root: &Path,
    event_type: &str,
    task_id: Option<&str>,
    payload: JsonValue,
) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    broker.with_conn(&db_path, "decapod", None, event_type, |conn| {
        ensure_schema(conn)?;
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: event_type.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            payload,
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })
}

/// Infer category from task title and tags by matching against known categories
fn infer_category_from_task(
    conn: &Connection,
    title: &str,
    tags: &str,
) -> Result<Option<String>, error::DecapodError> {
    let search_text = format!("{} {}", title.to_lowercase(), tags.to_lowercase());

    let mut stmt = conn
        .prepare("SELECT name, keywords FROM categories")
        .map_err(error::DecapodError::RusqliteError)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(error::DecapodError::RusqliteError)?;

    for row_result in rows {
        let (category_name, keywords) = row_result.map_err(error::DecapodError::RusqliteError)?;
        let keywords_list: Vec<&str> = keywords.split(',').map(|k| k.trim()).collect();

        for keyword in keywords_list {
            if !keyword.is_empty() && search_text.contains(keyword) {
                return Ok(Some(category_name));
            }
        }
    }

    Ok(None)
}

/// Find an agent that's currently working on tasks in this category
fn find_agent_for_category(
    conn: &Connection,
    category: &str,
    now_ts: &str,
) -> Result<Option<String>, error::DecapodError> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT agent_id FROM agent_category_claims WHERE category = ?",
            [category],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    if let Some(agent) = owner {
        if is_agent_stale(conn, &agent, now_ts, AGENT_EVICT_TIMEOUT_SECS)? {
            return Ok(None);
        }
        return Ok(Some(agent));
    }

    let agent: Option<String> = conn
        .query_row(
            "SELECT assigned_to FROM tasks
             WHERE category = ?
             AND assigned_to != ''
             AND status NOT IN ('done', 'archived')
             LIMIT 1",
            [category],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;

    Ok(agent)
}

fn claim_category_if_unowned(
    conn: &Connection,
    category: &str,
    agent_id: &str,
    ts: &str,
) -> Result<(), error::DecapodError> {
    if category.is_empty() || agent_id.is_empty() {
        return Ok(());
    }

    conn.execute(
        "INSERT OR IGNORE INTO agent_category_claims(id, agent_id, category, claimed_at, updated_at)
         VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![Ulid::new().to_string(), agent_id, category, ts, ts],
    )
    .map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}

fn get_category_owner(
    conn: &Connection,
    category: &str,
) -> Result<Option<String>, error::DecapodError> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT agent_id FROM agent_category_claims WHERE category = ?",
            [category],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(owner)
}

fn parse_owners_input(owners: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for owner in owners
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
    {
        if seen.insert(owner.clone()) {
            out.push(owner);
        }
    }
    out
}

fn upsert_task_owner(
    conn: &Connection,
    task_id: &str,
    agent_id: &str,
    claim_type: &str,
    ts: &str,
) -> Result<String, error::DecapodError> {
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM task_owners WHERE task_id = ?1 AND agent_id = ?2 ORDER BY claimed_at LIMIT 1",
            rusqlite::params![task_id, agent_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;

    if let Some(id) = existing_id {
        conn.execute(
            "UPDATE task_owners SET claim_type = ?1, claimed_at = ?2 WHERE id = ?3",
            rusqlite::params![claim_type, ts, id],
        )
        .map_err(error::DecapodError::RusqliteError)?;
        Ok(id)
    } else {
        let claim_id = Ulid::new().to_string();
        conn.execute(
            "INSERT INTO task_owners(id, task_id, agent_id, claimed_at, claim_type)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![claim_id, task_id, agent_id, ts, claim_type],
        )
        .map_err(error::DecapodError::RusqliteError)?;
        Ok(claim_id)
    }
}

struct OwnershipClaimRecord<'a> {
    task_id: &'a str,
    agent_id: &'a str,
    claim_type: &'a str,
    claim_id: &'a str,
    actor: &'a str,
    ts: &'a str,
}

fn write_ownership_claim_event(
    root: &Path,
    conn: &Connection,
    claim: &OwnershipClaimRecord<'_>,
) -> Result<(), error::DecapodError> {
    let ev = TodoEvent {
        ts: claim.ts.to_string(),
        event_id: Ulid::new().to_string(),
        event_type: "ownership.claim".to_string(),
        task_id: Some(claim.task_id.to_string()),
        payload: serde_json::json!({
            "agent_id": claim.agent_id,
            "claim_type": claim.claim_type,
            "claim_id": claim.claim_id,
        }),
        actor: claim.actor.to_string(),
    };
    append_event(root, &ev)?;
    insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}

fn fetch_task_owners(
    conn: &Connection,
    task_id: &str,
) -> Result<Vec<TaskOwner>, error::DecapodError> {
    let mut stmt = conn
        .prepare(
            "SELECT agent_id, claim_type, claimed_at FROM task_owners WHERE task_id = ? ORDER BY claimed_at",
        )
        .map_err(error::DecapodError::RusqliteError)?;
    let rows = stmt
        .query_map([task_id], |row| {
            Ok(TaskOwner {
                agent_id: row.get(0)?,
                claim_type: row.get(1)?,
                claimed_at: row.get(2)?,
            })
        })
        .map_err(error::DecapodError::RusqliteError)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(error::DecapodError::RusqliteError)?);
    }
    Ok(out)
}

fn primary_owner_from_owners(owners: &[TaskOwner]) -> Option<String> {
    owners
        .iter()
        .find(|o| o.claim_type == "primary")
        .or_else(|| owners.first())
        .map(|o| o.agent_id.clone())
}

fn sync_legacy_owner_column(conn: &Connection, task_id: &str) -> Result<(), error::DecapodError> {
    let owners = fetch_task_owners(conn, task_id)?;
    let primary_owner = primary_owner_from_owners(&owners).unwrap_or_default();
    conn.execute(
        "UPDATE tasks SET owner = ?1 WHERE id = ?2",
        rusqlite::params![primary_owner, task_id],
    )
    .map_err(error::DecapodError::RusqliteError)?;
    Ok(())
}

fn set_task_owners(
    root: &Path,
    conn: &Connection,
    task_id: &str,
    owners: &[String],
    actor: &str,
    ts: &str,
) -> Result<(), error::DecapodError> {
    let existing = fetch_task_owners(conn, task_id)?;
    let existing_agents: HashSet<String> = existing.iter().map(|o| o.agent_id.clone()).collect();
    let desired_agents: HashSet<String> = owners.iter().cloned().collect();

    for removed_agent in existing_agents.difference(&desired_agents) {
        conn.execute(
            "DELETE FROM task_owners WHERE task_id = ?1 AND agent_id = ?2",
            rusqlite::params![task_id, removed_agent],
        )
        .map_err(error::DecapodError::RusqliteError)?;
        let ev = TodoEvent {
            ts: ts.to_string(),
            event_id: Ulid::new().to_string(),
            event_type: "ownership.release".to_string(),
            task_id: Some(task_id.to_string()),
            payload: serde_json::json!({
                "agent_id": removed_agent,
            }),
            actor: actor.to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
    }

    for (idx, agent_id) in owners.iter().enumerate() {
        let claim_type = if idx == 0 { "primary" } else { "secondary" };
        let claim_id = upsert_task_owner(conn, task_id, agent_id, claim_type, ts)?;
        write_ownership_claim_event(
            root,
            conn,
            &OwnershipClaimRecord {
                task_id,
                agent_id,
                claim_type,
                claim_id: &claim_id,
                actor,
                ts,
            },
        )?;
    }

    sync_legacy_owner_column(conn, task_id)?;
    Ok(())
}

pub fn add_task(root: &Path, args: &TodoCommand) -> Result<serde_json::Value, error::DecapodError> {
    let TodoCommand::Add {
        title,
        description,
        priority,
        tags,
        owner,
        due,
        r#ref,
        dir,
        depends_on,
        blocks,
        parent,
    } = args
    else {
        return Err(error::DecapodError::ValidationError(
            "invalid command".into(),
        ));
    };

    let dir_path = dir
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());
    let dir_abs = Path::new(&dir_path)
        .canonicalize()
        .map_err(error::DecapodError::IoError)?
        .to_string_lossy()
        .to_string();
    let scope = scope_from_dir(&dir_abs);
    let prefix = task_prefix_from_scope(&scope);
    let task_id = format!("{}_{}", prefix, Ulid::new());
    let ts = now_iso();
    let intent_ref = format!("intent:todo.add:{}", Ulid::new());
    let owner_list = parse_owners_input(owner);
    let primary_owner = owner_list.first().cloned().unwrap_or_default();

    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", Some(&intent_ref), "todo.add", |conn| {
        ensure_schema(conn)?;

        // Infer category from tags or title for auto-assignment
        let inferred_category = infer_category_from_task(conn, title, tags)?;

        // Check if there's an agent already working on tasks in this category
        let auto_assigned_agent = if let Some(cat) = &inferred_category {
            find_agent_for_category(conn, cat, &ts)?
        } else {
            None
        };

        // Determine assigned_to and assigned_at
        let (assigned_to, assigned_at) = if let Some(agent) = auto_assigned_agent {
            (agent, Some(ts.clone()))
        } else {
            (String::new(), None)
        };


        if let Some(cat) = inferred_category.as_deref() {
            if !assigned_to.is_empty() {
                claim_category_if_unowned(conn, cat, &assigned_to, &ts)?;
            }
        }

        conn.execute(
            "INSERT INTO tasks(id, title, description, tags, owner, due, ref, status, created_at, updated_at, completed_at, closed_at, dir_path, scope, parent_task_id, priority, depends_on, blocks, category, assigned_to, assigned_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?9, NULL, NULL, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            rusqlite::params![
                task_id,
                title,
                description,
                tags,
                primary_owner,
                due,
                r#ref,
                ts,
                ts,
                dir_abs,
                scope,
                parent,
                priority,
                depends_on,
                blocks,
                inferred_category.clone().unwrap_or_default(),
                assigned_to,
                assigned_at
            ],
        )?;

        let mut payload = serde_json::json!({
            "intent_ref": intent_ref,
            "title": title,
            "description": description,
            "tags": tags,
            "owner": primary_owner,
            "owners": owner_list.clone(),
            "due": due,
            "ref": r#ref,
            "dir_path": dir_abs,
            "scope": scope,
            "parent_task_id": parent,
            "priority": priority,
            "depends_on": depends_on,
            "blocks": blocks,
            "category": inferred_category.clone().unwrap_or_default(),
        });

        // Add auto-assignment info if applicable
        if !assigned_to.is_empty() {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("assigned_to".to_string(), serde_json::json!(assigned_to));
                obj.insert("auto_assigned".to_string(), serde_json::json!(true));
            }
        }

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.add".to_string(),
            task_id: Some(task_id.clone()),
            payload,
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

        for (idx, owner_agent) in owner_list.iter().enumerate() {
            let claim_type = if idx == 0 { "primary" } else { "secondary" };
            let claim_id = upsert_task_owner(conn, &task_id, owner_agent, claim_type, &ts)?;
            write_ownership_claim_event(
                root,
                conn,
                &OwnershipClaimRecord {
                    task_id: &task_id,
                    agent_id: owner_agent,
                    claim_type,
                    claim_id: &claim_id,
                    actor: "decapod",
                    ts: &ts,
                },
            )?;
        }
        sync_legacy_owner_column(conn, &task_id)?;
        Ok(())
    })?;

    // Create federation node for intent→change→proof chain
    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.to_path_buf(),
    };
    if let Err(e) = federation::add_node(
        &store,
        &format!("Task: {}", title),
        "commitment",
        "notable",
        "agent_inferred",
        &format!(
            "Task {} created with priority {}. Description: {}",
            task_id, priority, description
        ),
        &format!("event:{}", task_id),
        tags,
        "repo",
        None,
        "decapod",
    ) {
        eprintln!("Warning: failed to create federation node: {}", e);
    } else {
        // Refresh derived files after adding a node
        let _ = federation::refresh_derived_files(&store);
    }

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.add",
        "status": "ok",
        "root": root.to_string_lossy(),
        "id": task_id,
    }))
}

pub fn update_status(
    store: &Store,
    id: &str,
    new_status: &str,
    event_type: &str,
    payload: JsonValue,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let intent_ref = format!("intent:{}:{}", event_type, Ulid::new());
    let root = &store.root;
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    // Risk Check
    let risk_map_path = root.join("RISKMAP.json");
    let risk_map = if risk_map_path.exists() {
        let content = std::fs::read_to_string(risk_map_path)?;
        serde_json::from_str(&content).unwrap_or(policy::RiskMap { zones: vec![] })
    } else {
        policy::RiskMap { zones: vec![] }
    };
    let (level, _) = policy::eval_risk(event_type, None, &risk_map);
    let requires_human =
        policy::human_in_loop_required(store, "global", level, policy::is_high_risk(level));
    if requires_human && !policy::check_approval(store, event_type, None, "global")? {
        return Err(error::DecapodError::ValidationError(format!(
            "Action '{}' on '{}' is high risk and lacks approval.",
            event_type, id
        )));
    }

    let mut payload = payload;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "intent_ref".to_string(),
            serde_json::json!(intent_ref.clone()),
        );
    }

    let changed = broker.with_conn(&db_path, "decapod", Some(&intent_ref), event_type, |conn| {
        ensure_schema(conn)?;
        let changed = conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2, completed_at = CASE WHEN ?1 = 'done' THEN ?2 ELSE completed_at END WHERE id = ?3",
            rusqlite::params![new_status, ts, id],
        )?;

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: event_type.to_string(),
            task_id: Some(id.to_string()),
            payload: payload.clone(),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(changed)
    })?;

    // Create federation node for proof when task is completed and link to intent
    if new_status == "done" && changed > 0 {
        // Find the original intent node (created at task.add)
        let intent_source = format!("event:{}", id);
        let intent_node_id = federation::find_node_by_source(store, &intent_source)
            .ok()
            .flatten();

        // Create the proof node
        let proof_result = federation::add_node(
            store,
            &format!("Proof: Task {} completed", id),
            "decision",
            "notable",
            "agent_inferred",
            &format!("Task {} marked as done. Validation gates passed.", id),
            &intent_source,
            "proof,completion",
            "repo",
            None,
            "decapod",
        );

        // If we found the intent node and proof node was created, link them
        if let (Ok(proof), Some(intent_id)) = (proof_result, intent_node_id) {
            let _ = federation::add_edge(store, &intent_id, &proof.id, "depends_on");
        }

        // Refresh derived files after adding proof node
        let _ = federation::refresh_derived_files(store);
    }

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": event_type,
        "status": if changed > 0 { "ok" } else { "not_found" },
        "root": root.to_string_lossy(),
        "id": id,
    }))
}

fn comment_task(
    root: &Path,
    id: &str,
    comment: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.comment", |conn| {
        ensure_schema(conn)?;
        // Event-only; does not mutate task row.
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.comment".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::json!({ "comment": comment }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;
        Ok(())
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.comment",
        "status": "ok",
        "root": root.to_string_lossy(),
        "id": id,
    }))
}

fn edit_task(
    root: &Path,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
    owner: Option<&str>,
    category: Option<&str>,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    let changed = broker.with_conn(&db_path, "decapod", None, "todo.edit", |conn| {
        ensure_schema(conn)?;

        // Validate category if provided
        if let Some(cat) = category {
            if !cat.is_empty() {
                let valid: bool = conn
                    .query_row(
                        "SELECT 1 FROM categories WHERE name = ?",
                        [cat],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(error::DecapodError::RusqliteError)?
                    .unwrap_or(false);
                if !valid {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Unknown category '{}'. Run `decapod todo categories` to see valid categories.",
                        cat
                    )));
                }
            }
        }

        // Build update fields and track what changed
        let mut updates = vec![];
        let mut params: Vec<Box<dyn ToSql>> = vec![];

        if let Some(t) = title {
            updates.push("title = ?");
            params.push(Box::new(t.to_string()));
        }

        if let Some(d) = description {
            updates.push("description = ?");
            params.push(Box::new(d.to_string()));
        }

        if let Some(c) = category {
            updates.push("category = ?");
            params.push(Box::new(c.to_string()));
        }

        if updates.is_empty() && owner.is_none() {
            return Ok(0usize);
        }

        let changed = if updates.is_empty() {
            conn.execute(
                "UPDATE tasks SET updated_at = ? WHERE id = ?",
                rusqlite::params![ts, id],
            )?
        } else {
            // Always update updated_at
            let sql = format!(
                "UPDATE tasks SET {}, updated_at = ? WHERE id = ?",
                updates.join(", ")
            );
            params.push(Box::new(ts.clone()));
            params.push(Box::new(id.to_string()));
            conn.execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            )?
        };

        // Create edit event
        let mut payload = serde_json::Map::new();
        if let Some(t) = title {
            payload.insert("title".to_string(), serde_json::json!(t));
        }
        if let Some(d) = description {
            payload.insert("description".to_string(), serde_json::json!(d));
        }
        if let Some(o) = owner {
            payload.insert("owner".to_string(), serde_json::json!(o));
        }
        if let Some(c) = category {
            payload.insert("category".to_string(), serde_json::json!(c));
        }

        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.edit".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::Value::Object(payload),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

        if let Some(o) = owner {
            let owner_list = parse_owners_input(o);
            set_task_owners(root, conn, id, &owner_list, "decapod", &ts)?;
        }

        Ok(changed)
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.edit",
        "status": if changed > 0 { "ok" } else { "not_found" },
        "root": root.to_string_lossy(),
        "id": id,
    }))
}

fn claim_task(
    root: &Path,
    id: &str,
    agent_id: &str,
    mode: ClaimMode,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    let result = broker.with_conn(&db_path, "decapod", None, "todo.claim", |conn| {
        ensure_schema(conn)?;
        touch_agent_presence(conn, agent_id, &ts)?;
        let claim_zone = if mode == ClaimMode::Shared {
            "todo.claim.shared"
        } else {
            "todo.claim.exclusive"
        };
        enforce_operation_policy(root, conn, claim_zone, agent_id)?;

        // Check if task exists and is not already claimed
        let current: Option<(String, String, String)> = conn
            .query_row(
                "SELECT status, assigned_to, category FROM tasks WHERE id = ?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(error::DecapodError::RusqliteError)?;

        match current {
            None => {
                return Ok(serde_json::json!({
                    "status": "not_found",
                    "message": format!("Task {} not found", id)
                }));
            }
            Some((status, assigned_to, category)) => {
                if status == "done" || status == "archived" {
                    return Ok(serde_json::json!({
                        "status": "error",
                        "message": format!("Task {} is already {}", id, status)
                    }));
                }
                if !assigned_to.is_empty() && assigned_to != agent_id {
                    if mode == ClaimMode::Shared {
                        let claim_id = upsert_task_owner(conn, id, agent_id, "secondary", &ts)?;
                        write_ownership_claim_event(
                            root,
                            conn,
                            &OwnershipClaimRecord {
                                task_id: id,
                                agent_id,
                                claim_type: "secondary",
                                claim_id: &claim_id,
                                actor: agent_id,
                                ts: &ts,
                            },
                        )?;
                        sync_legacy_owner_column(conn, id)?;
                        return Ok(serde_json::json!({
                            "status": "ok",
                            "mode": "shared",
                            "message": format!("Task {} is assigned to {}; added {} as secondary owner", id, assigned_to, agent_id),
                            "assigned_to": assigned_to,
                            "claim_id": claim_id
                        }));
                    }
                    return Ok(serde_json::json!({
                        "status": "conflict",
                        "mode": "exclusive",
                        "message": format!("Task {} is already claimed by {}", id, assigned_to),
                        "resolution": "none",
                        "assigned_to": assigned_to
                    }));
                }

                if !category.is_empty() && mode == ClaimMode::Exclusive {
                    if let Some(owner) = get_category_owner(conn, &category)? {
                        if owner != agent_id {
                            if is_agent_stale(conn, &owner, &ts, AGENT_EVICT_TIMEOUT_SECS)? {
                                conn.execute(
                                    "UPDATE agent_category_claims
                                     SET agent_id = ?, claimed_at = ?, updated_at = ?
                                     WHERE category = ?",
                                    rusqlite::params![agent_id, ts, ts, category],
                                )
                                .map_err(error::DecapodError::RusqliteError)?;
                            } else {
                                return Ok(serde_json::json!({
                                    "status": "error",
                                    "message": format!(
                                        "Category '{}' is owned by {}; cannot claim task {}",
                                        category, owner, id
                                    )
                                }));
                            }
                        }
                    } else {
                        claim_category_if_unowned(conn, &category, agent_id, &ts)?;
                    }
                }
            }
        }

        // Claim the task
        conn.execute(
            "UPDATE tasks SET assigned_to = ?, assigned_at = ?, updated_at = ? WHERE id = ?",
            [agent_id, &ts, &ts, id],
        )
        .map_err(error::DecapodError::RusqliteError)?;

        let claim_id = upsert_task_owner(conn, id, agent_id, "primary", &ts)?;
        write_ownership_claim_event(
            root,
            conn,
            &OwnershipClaimRecord {
                task_id: id,
                agent_id,
                claim_type: "primary",
                claim_id: &claim_id,
                actor: agent_id,
                ts: &ts,
            },
        )?;
        sync_legacy_owner_column(conn, id)?;

        // Create claim event
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.claim".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::json!({
                "assigned_to": agent_id,
                "mode": format!("{:?}", mode).to_lowercase(),
            }),
            actor: agent_id.to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

        Ok(serde_json::json!({
            "status": "ok",
            "mode": format!("{:?}", mode).to_lowercase(),
            "message": format!("Task {} claimed by {}", id, agent_id),
            "claim_id": claim_id
        }))
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.claim",
        "status": result.get("status").and_then(|v| v.as_str()).unwrap_or("error"),
        "root": root.to_string_lossy(),
        "id": id,
        "result": result,
    }))
}

fn handoff_task(
    store: &Store,
    id: &str,
    to: &str,
    from: Option<&str>,
    summary: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let root = &store.root;
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);
    let ts = now_iso();

    let result = broker.with_conn(&db_path, "decapod", None, "todo.handoff", |conn| {
        ensure_schema(conn)?;
        let acting_agent = from.unwrap_or("unknown");
        enforce_operation_policy(root, conn, "todo.handoff", acting_agent)?;
        touch_agent_presence(conn, to, &ts)?;

        let current: Option<(String, String, String)> = conn
            .query_row(
                "SELECT status, assigned_to, category FROM tasks WHERE id = ?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(error::DecapodError::RusqliteError)?;

        let Some((status, assigned_to, category)) = current else {
            return Ok((serde_json::json!({
                "status": "not_found",
                "message": format!("Task {} not found", id)
            }), String::new()));
        };
        if status == "done" || status == "archived" {
            return Ok((serde_json::json!({
                "status": "error",
                "message": format!("Task {} is already {}", id, status)
            }), String::new()));
        }
        if let Some(expected_from) = from {
            if !assigned_to.is_empty() && assigned_to != expected_from {
                return Ok((serde_json::json!({
                    "status": "error",
                    "message": format!("Task {} assigned_to is {}, expected {}", id, assigned_to, expected_from)
                }), String::new()));
            }
        }
        if !category.is_empty() {
            conn.execute(
                "INSERT INTO agent_category_claims(id, agent_id, category, claimed_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(category) DO UPDATE SET
                   agent_id = excluded.agent_id,
                   claimed_at = excluded.claimed_at,
                   updated_at = excluded.updated_at",
                rusqlite::params![Ulid::new().to_string(), to, category, ts, ts],
            )
            .map_err(error::DecapodError::RusqliteError)?;
        }

        conn.execute(
            "UPDATE tasks SET assigned_to = ?, assigned_at = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![to, ts, ts, id],
        )
        .map_err(error::DecapodError::RusqliteError)?;

        let previous = if assigned_to.is_empty() {
            from.unwrap_or("unassigned").to_string()
        } else {
            assigned_to
        };

        let event_id = Ulid::new().to_string();
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: event_id.clone(),
            event_type: "task.handoff".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::json!({
                "from": previous,
                "to": to,
                "summary": summary,
            }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

        Ok((serde_json::json!({
            "status": "ok",
            "message": format!("Task {} handed off to {}", id, to)
        }), event_id))
    })?;

    let (status_result, event_id): (serde_json::Value, String) = result;
    let mut reconcile_result = serde_json::json!({
        "status": "skipped",
        "reason": "handoff_not_ok"
    });
    if status_result
        .get("status")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s == "ok")
    {
        let knowledge_id = format!("H_{}", Ulid::new());
        let title = format!("Task handoff {}", id);
        let content = format!("Handoff from {:?} to {}. Summary: {}", from, to, summary);
        let provenance = format!("event:{}", event_id);
        let _ = knowledge::add_knowledge(store, &knowledge_id, &title, &content, &provenance, None);
        let obs = format!("Task {} handoff to {}: {}", id, to, summary);
        let _ = teammate::record_observation(store, &obs, Some("multi_agent"));

        // Attempt cross-branch reconciliation: commit current changes and mirror to target agent branch.
        let repo_root = root
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| root.clone());
        reconcile_result = reconcile_commit_to_agent_branch(&repo_root, id, to, summary)?;
    }

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.handoff",
        "status": status_result.get("status").and_then(|v| v.as_str()).unwrap_or("error"),
        "root": root.to_string_lossy(),
        "id": id,
        "result": status_result,
        "event_id": if event_id.is_empty() { None::<String> } else { Some(event_id) },
        "reconcile": reconcile_result,
    }))
}

fn add_task_owner(
    root: &Path,
    task_id: &str,
    agent_id: &str,
    claim_type: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.add_owner", |conn| {
        ensure_schema(conn)?;

        // Verify task exists
        let exists: bool = conn
            .query_row("SELECT 1 FROM tasks WHERE id = ?", [task_id], |_| Ok(true))
            .unwrap_or(false);

        if !exists {
            return Ok(serde_json::json!({
                "status": "not_found",
                "message": format!("Task {} not found", task_id)
            }));
        }

        // Check for conflict - primary owner already exists
        if claim_type == "primary" {
            let existing_primary: Option<String> = conn
                .query_row(
                    "SELECT agent_id FROM task_owners WHERE task_id = ? AND claim_type = 'primary'",
                    [task_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(error::DecapodError::RusqliteError)?;

            if let Some(primary_agent) = existing_primary {
                if primary_agent != agent_id {
                return Ok(serde_json::json!({
                    "status": "conflict",
                    "message": "Task already has a primary owner. Use 'secondary' or resolve conflict."
                }));
                }
            }
        }

        let claim_id = upsert_task_owner(conn, task_id, agent_id, claim_type, &ts)?;
        write_ownership_claim_event(
            root,
            conn,
            &OwnershipClaimRecord {
                task_id,
                agent_id,
                claim_type,
                claim_id: &claim_id,
                actor: "decapod",
                ts: &ts,
            },
        )?;

        Ok(serde_json::json!({
            "ts": ts,
            "cmd": "todo.add_owner",
            "status": "ok",
            "root": root.to_string_lossy(),
            "task_id": task_id,
            "agent_id": agent_id,
            "claim_type": claim_type,
            "claim_id": claim_id,
        }))
    })
}

fn remove_task_owner(
    root: &Path,
    task_id: &str,
    agent_id: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.remove_owner", |conn| {
        ensure_schema(conn)?;

        let deleted = conn.execute(
            "DELETE FROM task_owners WHERE task_id = ?1 AND agent_id = ?2",
            rusqlite::params![task_id, agent_id],
        )?;

        if deleted == 0 {
            return Ok(serde_json::json!({
                "status": "not_found",
                "message": format!("Owner {} not found for task {}", agent_id, task_id)
            }));
        }

        // Log ownership release event
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "ownership.release".to_string(),
            task_id: Some(task_id.to_string()),
            payload: serde_json::json!({
                "agent_id": agent_id,
            }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev)?;
        sync_legacy_owner_column(conn, task_id)?;

        Ok(serde_json::json!({
            "ts": ts,
            "cmd": "todo.remove_owner",
            "status": "ok",
            "root": root.to_string_lossy(),
            "task_id": task_id,
            "agent_id": agent_id,
        }))
    })
}

fn list_task_owners(
    root: &Path,
    task_id: &str,
) -> Result<Vec<serde_json::Value>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.list_owners", |conn| {
        ensure_schema(conn)?;
        let owners = fetch_task_owners(conn, task_id)?;
        Ok(owners
            .into_iter()
            .map(|owner| {
                serde_json::json!({
                    "agent_id": owner.agent_id,
                    "claim_type": owner.claim_type,
                    "claimed_at": owner.claimed_at,
                })
            })
            .collect())
    })
}

fn register_agent_expertise(
    root: &Path,
    agent_id: &str,
    category: &str,
    level: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.register_expertise", |conn| {
        ensure_schema(conn)?;

        conn.execute(
            "INSERT INTO agent_expertise(id, agent_id, category, expertise_level, claimed_at, updated_at)
             VALUES(lower(hex(randomblob(16))), ?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(agent_id, category) DO UPDATE SET
               expertise_level = excluded.expertise_level,
               updated_at = excluded.updated_at",
            rusqlite::params![agent_id, category, level, ts],
        )?;

        // Log expertise event
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "agent.expertise".to_string(),
            task_id: None,
            payload: serde_json::json!({
                "agent_id": agent_id,
                "category": category,
                "expertise_level": level,
            }),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev)?;

        Ok(serde_json::json!({
            "ts": ts,
            "cmd": "todo.register_expertise",
            "status": "ok",
            "root": root.to_string_lossy(),
            "agent_id": agent_id,
            "category": category,
            "expertise_level": level,
        }))
    })
}

fn list_agent_expertise(
    root: &Path,
    agent_filter: Option<&str>,
    category_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.expertise", |conn| {
        ensure_schema(conn)?;

        // Handle the four cases of optional filters
        let expertise: Vec<serde_json::Value> = match (agent_filter, category_filter) {
            (Some(agent), Some(category)) => {
                let mut stmt = conn.prepare(
                    "SELECT agent_id, category, expertise_level, claimed_at, updated_at 
                     FROM agent_expertise 
                     WHERE agent_id = ? AND category = ?
                     ORDER BY agent_id, category",
                )?;
                stmt.query_map([agent, category], |row| {
                    Ok(serde_json::json!({
                        "agent_id": row.get::<_, String>(0)?,
                        "category": row.get::<_, String>(1)?,
                        "expertise_level": row.get::<_, String>(2)?,
                        "claimed_at": row.get::<_, String>(3)?,
                        "updated_at": row.get::<_, String>(4)?,
                    }))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
            (Some(agent), None) => {
                let mut stmt = conn.prepare(
                    "SELECT agent_id, category, expertise_level, claimed_at, updated_at 
                     FROM agent_expertise 
                     WHERE agent_id = ?
                     ORDER BY agent_id, category",
                )?;
                stmt.query_map([agent], |row| {
                    Ok(serde_json::json!({
                        "agent_id": row.get::<_, String>(0)?,
                        "category": row.get::<_, String>(1)?,
                        "expertise_level": row.get::<_, String>(2)?,
                        "claimed_at": row.get::<_, String>(3)?,
                        "updated_at": row.get::<_, String>(4)?,
                    }))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
            (None, Some(category)) => {
                let mut stmt = conn.prepare(
                    "SELECT agent_id, category, expertise_level, claimed_at, updated_at 
                     FROM agent_expertise 
                     WHERE category = ?
                     ORDER BY agent_id, category",
                )?;
                stmt.query_map([category], |row| {
                    Ok(serde_json::json!({
                        "agent_id": row.get::<_, String>(0)?,
                        "category": row.get::<_, String>(1)?,
                        "expertise_level": row.get::<_, String>(2)?,
                        "claimed_at": row.get::<_, String>(3)?,
                        "updated_at": row.get::<_, String>(4)?,
                    }))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT agent_id, category, expertise_level, claimed_at, updated_at 
                     FROM agent_expertise 
                     ORDER BY agent_id, category",
                )?;
                stmt.query_map([], |row| {
                    Ok(serde_json::json!({
                        "agent_id": row.get::<_, String>(0)?,
                        "category": row.get::<_, String>(1)?,
                        "expertise_level": row.get::<_, String>(2)?,
                        "claimed_at": row.get::<_, String>(3)?,
                        "updated_at": row.get::<_, String>(4)?,
                    }))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
        };

        Ok(expertise)
    })
}

fn release_task(root: &Path, id: &str) -> Result<serde_json::Value, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    let result = broker.with_conn(&db_path, "decapod", None, "todo.release", |conn| {
        ensure_schema(conn)?;

        // Check if task exists
        let exists: Option<String> = conn
            .query_row("SELECT assigned_to FROM tasks WHERE id = ?", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(error::DecapodError::RusqliteError)?;

        if exists.is_none() {
            return Ok(serde_json::json!({
                "status": "not_found",
                "message": format!("Task {} not found", id)
            }));
        }

        // Release the task
        conn.execute(
            "UPDATE tasks SET assigned_to = '', assigned_at = NULL, updated_at = ? WHERE id = ?",
            [&ts, id],
        )
        .map_err(error::DecapodError::RusqliteError)?;

        // Create release event
        let ev = TodoEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            event_type: "task.release".to_string(),
            task_id: Some(id.to_string()),
            payload: serde_json::json!({}),
            actor: "decapod".to_string(),
        };
        append_event(root, &ev)?;
        insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

        Ok(serde_json::json!({
            "status": "ok",
            "message": format!("Task {} released", id)
        }))
    })?;

    Ok(serde_json::json!({
        "ts": ts,
        "cmd": "todo.release",
        "status": result.get("status").and_then(|v| v.as_str()).unwrap_or("error"),
        "root": root.to_string_lossy(),
        "id": id,
        "result": result,
    }))
}

pub fn get_task(root: &Path, id: &str) -> Result<Option<Task>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.get", |conn| {
        ensure_schema(conn)?;
        let mut stmt = conn.prepare("SELECT id,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at FROM tasks WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            let task_id: String = row.get(0)?;
            let owners = fetch_task_owners(conn, &task_id)?;
            Ok(Some(Task {
                id: task_id,
                title: row.get(1)?,
                description: row.get(2)?,
                tags: row.get(3)?,
                owner: primary_owner_from_owners(&owners).unwrap_or_else(|| row.get(4).unwrap_or_default()),
                due: row.get(5)?,
                r#ref: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                completed_at: row.get(10)?,
                closed_at: row.get(11)?,
                dir_path: row.get(12)?,
                scope: row.get(13)?,
                parent_task_id: row.get(14)?,
                priority: row.get(15)?,
                depends_on: row.get(16)?,
                blocks: row.get(17)?,
                category: row.get(18)?,
                component: row.get(19)?,
                assigned_to: row.get(20).unwrap_or_default(),
                assigned_at: row.get(21)?,
                owners,
            }))
        } else {
            Ok(None)
        }
    })
}

pub fn list_tasks(
    root: &Path,
    status: Option<String>,
    scope: Option<String>,
    tags: Option<String>,
    title_search: Option<String>,
    dir: Option<String>,
) -> Result<Vec<Task>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = todo_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "todo.list", |conn| {
        ensure_schema(conn)?;

        let mut query = "SELECT id,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at FROM tasks WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(s) = status {
            query.push_str(" AND status = ?");
            params.push(Box::new(s));
        }
        if let Some(s) = scope {
            query.push_str(" AND scope = ?");
            params.push(Box::new(s));
        }
        if let Some(t) = tags {
            query.push_str(" AND tags LIKE ?");
            params.push(Box::new(format!("%{}%", t)));
        }
        if let Some(ts) = title_search {
            query.push_str(" AND title LIKE ?");
            params.push(Box::new(format!("%{}%", ts)));
        }
        if let Some(d) = dir {
            let abs = Path::new(&d)
                .canonicalize()
                .map_err(error::DecapodError::IoError)?
                .to_string_lossy()
                .to_string();
            query.push_str(" AND dir_path = ?");
            params.push(Box::new(abs));
        }

        query.push_str(" ORDER BY updated_at DESC");

        let mut stmt = conn.prepare(&query)?;
        let params_as_dyn: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params_as_dyn.iter().copied()))
            .map_err(error::DecapodError::RusqliteError)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(error::DecapodError::RusqliteError)? {
            let task_id: String = row.get(0).map_err(error::DecapodError::RusqliteError)?;
            let owners = fetch_task_owners(conn, &task_id)?;
            out.push(Task {
                id: task_id,
                title: row.get(1).map_err(error::DecapodError::RusqliteError)?,
                description: row.get(2).map_err(error::DecapodError::RusqliteError)?,
                tags: row.get(3).map_err(error::DecapodError::RusqliteError)?,
                owner: primary_owner_from_owners(&owners).unwrap_or_else(|| row.get(4).unwrap_or_default()),
                due: row.get(5).map_err(error::DecapodError::RusqliteError)?,
                r#ref: row.get(6).map_err(error::DecapodError::RusqliteError)?,
                status: row.get(7).map_err(error::DecapodError::RusqliteError)?,
                created_at: row.get(8).map_err(error::DecapodError::RusqliteError)?,
                updated_at: row.get(9).map_err(error::DecapodError::RusqliteError)?,
                completed_at: row.get(10).map_err(error::DecapodError::RusqliteError)?,
                closed_at: row.get(11).map_err(error::DecapodError::RusqliteError)?,
                dir_path: row.get(12).map_err(error::DecapodError::RusqliteError)?,
                scope: row.get(13).map_err(error::DecapodError::RusqliteError)?,
                parent_task_id: row.get(14).map_err(error::DecapodError::RusqliteError)?,
                priority: row.get(15).map_err(error::DecapodError::RusqliteError)?,
                depends_on: row.get(16).map_err(error::DecapodError::RusqliteError)?,
                blocks: row.get(17).map_err(error::DecapodError::RusqliteError)?,
                category: row.get(18).map_err(error::DecapodError::RusqliteError)?,
                component: row.get(19).map_err(error::DecapodError::RusqliteError)?,
                assigned_to: row
                    .get(20)
                    .map_err(error::DecapodError::RusqliteError)
                    .unwrap_or_default(),
                assigned_at: row.get(21).map_err(error::DecapodError::RusqliteError)?,
                owners,
            });
        }
        Ok(out)
    })
}

pub fn rebuild_from_events(root: &Path) -> Result<serde_json::Value, error::DecapodError> {
    let ev_path = events_path(root);
    if !ev_path.is_file() {
        // Empty store is valid; create empty DB with schema.
        let conn = connect_todo(root)?;
        ensure_schema(&conn)?;
        return Ok(serde_json::json!({
            "ts": now_iso(),
            "cmd": "todo.rebuild",
            "status": "ok",
            "root": root.to_string_lossy(),
            "events": 0,
            "note": "no events file; created empty DB"
        }));
    }

    // Rebuild into a temp DB then swap into place for atomicity.
    let tmp_db = root.join(format!(".{}.tmp", schemas::TODO_DB_NAME));
    if tmp_db.exists() {
        fs::remove_file(&tmp_db).map_err(error::DecapodError::IoError)?;
    }

    let count = rebuild_db_from_events(&ev_path, &tmp_db)?;

    // Swap
    let final_db = todo_db_path(root);
    if final_db.exists() {
        fs::remove_file(&final_db).map_err(error::DecapodError::IoError)?;
    }
    fs::rename(&tmp_db, &final_db).map_err(error::DecapodError::IoError)?;

    Ok(serde_json::json!({
        "ts": now_iso(),
        "cmd": "todo.rebuild",
        "status": "ok",
        "root": root.to_string_lossy(),
        "events": count,
    }))
}

pub fn rebuild_db_from_events(events: &Path, out_db: &Path) -> Result<u64, error::DecapodError> {
    let broker = DbBroker::new(out_db.parent().unwrap());

    broker.with_conn(out_db, "decapod", None, "todo.rebuild_internal", |conn| {
        ensure_schema(conn)?;

        let f = OpenOptions::new()
            .read(true)
            .open(events)
            .map_err(error::DecapodError::IoError)?;
        let reader = BufReader::new(f);

        let mut count = 0u64;
        for line in reader.lines() {
            let line = line.map_err(error::DecapodError::IoError)?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let ev: TodoEvent = serde_json::from_str(line).map_err(|e| {
                error::DecapodError::ValidationError(format!("Invalid JSONL event: {}", e))
            })?;
            count += 1;

            insert_event(conn, &ev).map_err(error::DecapodError::RusqliteError)?;

            match ev.event_type.as_str() {
                "task.add" => {
                    let id = ev.task_id.clone().ok_or_else(|| {
                        error::DecapodError::ValidationError("task.add missing task_id".into())
                    })?;
                    let title = ev
                        .payload
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tags = ev
                        .payload
                        .get("tags")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let description = ev
                        .payload
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let owner = ev
                        .payload
                        .get("owner")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let due = ev
                        .payload
                        .get("due")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let r#ref = ev
                        .payload
                        .get("ref")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let dir_path = ev
                        .payload
                        .get("dir_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let scope = ev
                        .payload
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("root")
                        .to_string();
                    let parent_task_id = ev
                        .payload
                        .get("parent_task_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let priority = ev
                        .payload
                        .get("priority")
                        .and_then(|v| v.as_str())
                        .unwrap_or("medium")
                        .to_string();
                    let depends_on = ev
                        .payload
                        .get("depends_on")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let blocks = ev
                        .payload
                        .get("blocks")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let category = ev
                        .payload
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let component = ev
                        .payload
                        .get("component")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let assigned_to = ev
                        .payload
                        .get("assigned_to")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let assigned_at = if assigned_to.is_empty() {
                        None
                    } else {
                        Some(ev.ts.clone())
                    };

                    conn.execute(
                        "INSERT OR REPLACE INTO tasks(id,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,'open',?8,?9,NULL,NULL,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
                        rusqlite::params![id, title, description, tags, owner, due, r#ref, ev.ts, ev.ts, dir_path, scope, parent_task_id, priority, depends_on, blocks, category, component, assigned_to, assigned_at],
                    )?;

                    if let Some(owners) = ev.payload.get("owners").and_then(|v| v.as_array()) {
                        for (idx, owner_value) in owners.iter().enumerate() {
                            if let Some(owner_agent) = owner_value.as_str() {
                                if owner_agent.is_empty() {
                                    continue;
                                }
                                let claim_type = if idx == 0 { "primary" } else { "secondary" };
                                let _ =
                                    upsert_task_owner(conn, &id, owner_agent, claim_type, &ev.ts)?;
                            }
                        }
                    } else if !owner.is_empty() {
                        let _ = upsert_task_owner(conn, &id, &owner, "primary", &ev.ts)?;
                    }
                }
                "task.done" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    conn.execute(
                        "UPDATE tasks SET status='done', updated_at=?1, completed_at=?1 WHERE id=?2",
                        rusqlite::params![ev.ts, id],
                    )?;
                }
                "task.archive" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    conn.execute(
                        "UPDATE tasks SET status='archived', updated_at=?1, closed_at=?1 WHERE id=?2",
                        rusqlite::params![ev.ts, id],
                    )?;
                }
                "task.comment" => {}
                "task.edit" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    if let Some(title) = ev.payload.get("title").and_then(|v| v.as_str()) {
                        conn.execute(
                            "UPDATE tasks SET title = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![title, ev.ts, id],
                        )?;
                    }
                    if let Some(description) =
                        ev.payload.get("description").and_then(|v| v.as_str())
                    {
                        conn.execute(
                            "UPDATE tasks SET description = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![description, ev.ts, id],
                        )?;
                    }
                    if let Some(owner) = ev.payload.get("owner").and_then(|v| v.as_str()) {
                        conn.execute(
                            "UPDATE tasks SET owner = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![owner, ev.ts, id],
                        )?;
                    }
                }
                "task.claim" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    let assigned_to = ev
                        .payload
                        .get("assigned_to")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    conn.execute(
                        "UPDATE tasks SET assigned_to = ?1, assigned_at = ?2, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![assigned_to, ev.ts, id],
                    )?;
                }
                "task.release" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    conn.execute(
                        "UPDATE tasks SET assigned_to = '', assigned_at = NULL, updated_at = ?1 WHERE id = ?2",
                        rusqlite::params![ev.ts, id],
                    )?;
                }
                "task.handoff" => {
                    let id = ev.task_id.clone().unwrap_or_default();
                    let to = ev.payload.get("to").and_then(|v| v.as_str()).unwrap_or("");
                    conn.execute(
                        "UPDATE tasks SET assigned_to = ?1, assigned_at = ?2, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![to, ev.ts, id],
                    )?;
                }

                "agent.heartbeat" => {
                    let agent_id = ev
                        .payload
                        .get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&ev.actor);

                    conn.execute(
                        "INSERT INTO agent_presence(agent_id, last_seen, status, updated_at)
                         VALUES(?1, ?2, 'active', ?2)
                         ON CONFLICT(agent_id) DO UPDATE SET
                           last_seen = excluded.last_seen,
                           status = 'active',
                           updated_at = excluded.updated_at",

                        rusqlite::params![agent_id, ev.ts],
                    )?;
                }
                "ownership.claim" => {
                    let task_id = ev.task_id.clone().unwrap_or_default();
                    let agent_id = ev.payload.get("agent_id").and_then(|v| v.as_str()).unwrap_or(&ev.actor);
                    let claim_type = ev.payload.get("claim_type").and_then(|v| v.as_str()).unwrap_or("secondary");
                    let claim_id = ev.payload.get("claim_id").and_then(|v| v.as_str()).unwrap_or("");
                    let existing_id: Option<String> = conn
                        .query_row(
                            "SELECT id FROM task_owners WHERE task_id = ?1 AND agent_id = ?2 ORDER BY claimed_at LIMIT 1",
                            rusqlite::params![task_id, agent_id],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(error::DecapodError::RusqliteError)?;
                    if let Some(existing_id) = existing_id {
                        conn.execute(
                            "UPDATE task_owners SET claim_type = ?1, claimed_at = ?2 WHERE id = ?3",
                            rusqlite::params![claim_type, ev.ts, existing_id],
                        )?;
                    } else {
                        let insert_id = if claim_id.is_empty() {
                            Ulid::new().to_string()
                        } else {
                            claim_id.to_string()
                        };
                        conn.execute(
                            "INSERT INTO task_owners(id, task_id, agent_id, claimed_at, claim_type)
                             VALUES(?1, ?2, ?3, ?4, ?5)",
                            rusqlite::params![insert_id, task_id, agent_id, ev.ts, claim_type],
                        )?;
                    }
                }
                "ownership.release" => {
                    let task_id = ev.task_id.clone().unwrap_or_default();
                    let agent_id = ev.payload.get("agent_id").and_then(|v| v.as_str()).unwrap_or(&ev.actor);
                    conn.execute(
                        "DELETE FROM task_owners WHERE task_id = ?1 AND agent_id = ?2",
                        rusqlite::params![task_id, agent_id],
                    )?;
                }
                "agent.expertise" => {
                    let agent_id = ev.payload.get("agent_id").and_then(|v| v.as_str()).unwrap_or(&ev.actor);
                    let category = ev.payload.get("category").and_then(|v| v.as_str()).unwrap_or("");
                    let expertise_level = ev.payload.get("expertise_level").and_then(|v| v.as_str()).unwrap_or("intermediate");
                    conn.execute(
                        "INSERT INTO agent_expertise(id, agent_id, category, expertise_level, claimed_at, updated_at)
                         VALUES(lower(hex(randomblob(16))), ?1, ?2, ?3, ?4, ?4)
                         ON CONFLICT(agent_id, category) DO UPDATE SET
                           expertise_level = excluded.expertise_level,
                           updated_at = excluded.updated_at",
                        rusqlite::params![agent_id, category, expertise_level, ev.ts],
                    )?;
                }
                "task.verify.capture" | "task.verify.result" => {
                    let id = ev.task_id.clone().ok_or_else(|| {
                        error::DecapodError::ValidationError(format!(
                            "{} missing task_id",
                            ev.event_type
                        ))
                    })?;
                    let proof_plan = ev
                        .payload
                        .get("proof_plan")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    let artifacts = ev
                        .payload
                        .get("verification_artifacts")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let last_verified_status = ev
                        .payload
                        .get("last_verified_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let last_verified_notes = ev
                        .payload
                        .get("last_verified_notes")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let verification_policy_days = ev
                        .payload
                        .get("verification_policy_days")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(90);

                    conn.execute(
                        "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
                         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?4)
                         ON CONFLICT(todo_id) DO UPDATE SET
                           proof_plan=excluded.proof_plan,
                           verification_artifacts=excluded.verification_artifacts,
                           last_verified_at=excluded.last_verified_at,
                           last_verified_status=excluded.last_verified_status,
                           last_verified_notes=excluded.last_verified_notes,
                           verification_policy_days=excluded.verification_policy_days,
                           updated_at=excluded.updated_at",
                        rusqlite::params![
                            id,
                            serde_json::to_string(&proof_plan).unwrap(),
                            if artifacts.is_null() {
                                None::<String>
                            } else {
                                Some(serde_json::to_string(&artifacts).unwrap())
                            },
                            ev.ts,
                            last_verified_status,
                            last_verified_notes,
                            verification_policy_days,
                        ],
                    )?;
                }
                _ => {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Unknown event_type '{}'",
                        ev.event_type
                    )));
                }
            }
        }
        Ok(count)
    })
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "todo",
        "version": "0.1.0",
        "description": "Manage TODO tasks",
        "commands": [
            { "name": "add", "parameters": ["title", "tags", "owner", "due", "ref", "dir", "priority", "depends_on", "blocks", "parent"] },
            { "name": "list", "parameters": ["status", "scope", "tags", "title_search", "dir"] },
            { "name": "get", "parameters": ["id"] },
            { "name": "done", "parameters": ["id", "validated", "artifact"] },
            { "name": "archive", "parameters": ["id"] },
            { "name": "comment", "parameters": ["id", "comment"] },
            { "name": "edit", "parameters": ["id", "title", "description", "owner"] },
            { "name": "claim", "parameters": ["id", "agent", "mode"] },
            { "name": "release", "parameters": ["id"] },
            { "name": "categories", "parameters": [] },
            { "name": "register-agent", "parameters": ["agent", "category"] },
            { "name": "ownerships", "parameters": ["category", "agent"] },
            { "name": "heartbeat", "parameters": ["agent"] },
            { "name": "presence", "parameters": ["agent"] },
            { "name": "handoff", "parameters": ["id", "to", "from", "summary"] },
            { "name": "add-owner", "parameters": ["id", "agent", "claim_type"] },
            { "name": "remove-owner", "parameters": ["id", "agent"] },
            { "name": "list-owners", "parameters": ["id"] },
            { "name": "register-expertise", "parameters": ["agent", "category", "level"] },
            { "name": "expertise", "parameters": ["agent", "category"] },

            { "name": "rebuild", "parameters": [] }
        ],
        "storage": ["todo.db", "todo.events.jsonl"]
    })
}

pub fn run_todo_cli(store: &Store, cli: TodoCli) -> Result<(), error::DecapodError> {
    let root = &store.root;
    let out = match &cli.command {
        TodoCommand::Add { .. } => add_task(root, &cli.command)?,
        TodoCommand::List {
            status,
            scope,
            tags,
            title_search,
            dir,
        } => {
            let items = list_tasks(
                root,
                status.clone(),
                scope.clone(),
                tags.clone(),
                title_search.clone(),
                dir.clone(),
            )?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.list",
                "status": "ok",
                "root": root.to_string_lossy(),
                "items": items,
            })
        }
        TodoCommand::Get { id } => {
            let t = get_task(root, id)?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.get",
                "status": if t.is_some() { "ok" } else { "not_found" },
                "root": root.to_string_lossy(),
                "item": t,
            })
        }
        TodoCommand::Done {
            id,
            validated,
            artifact,
        } => {
            let out = update_status(store, id, "done", "task.done", serde_json::json!({}))?;
            if *validated && out.get("status").and_then(|v| v.as_str()) == Some("ok") {
                let repo_root = store
                    .root
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .unwrap_or(std::env::current_dir().map_err(error::DecapodError::IoError)?);
                verify::capture_baseline_for_todo(store, &repo_root, id, artifact.clone())?;
            }
            out
        }
        TodoCommand::Archive { id } => {
            update_status(store, id, "archived", "task.archive", serde_json::json!({}))?
        }
        TodoCommand::Comment { id, comment } => comment_task(root, id, comment)?,
        TodoCommand::Edit {
            id,
            title,
            description,
            owner,
            category,
        } => edit_task(
            root,
            id,
            title.as_deref(),
            description.as_deref(),
            owner.as_deref(),
            category.as_deref(),
        )?,
        TodoCommand::Claim { id, agent, mode } => {
            let default_agent =
                env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let agent_id = agent.as_deref().unwrap_or(&default_agent);
            claim_task(root, id, agent_id, *mode)?
        }
        TodoCommand::Release { id } => release_task(root, id)?,
        TodoCommand::Rebuild => rebuild_from_events(root)?,
        TodoCommand::Categories => {
            let categories = list_categories(root)?;
            serde_json::json!({ "categories": categories })
        }
        TodoCommand::RegisterAgent { agent, categories } => {
            let default_agent =
                env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let agent_id = agent.as_deref().unwrap_or(&default_agent);
            register_agent_categories(root, agent_id, categories)?
        }
        TodoCommand::Ownerships { category, agent } => {
            let claims = list_category_ownerships(root, category.as_deref(), agent.as_deref())?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.ownerships",
                "status": "ok",
                "root": root.to_string_lossy(),
                "claims": claims,
            })
        }
        TodoCommand::Heartbeat { agent } => {
            let default_agent =
                env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let agent_id = agent.as_deref().unwrap_or(&default_agent);
            record_heartbeat(root, agent_id)?
        }
        TodoCommand::Presence { agent } => {
            let agents = list_agent_presence(root, agent.as_deref())?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.presence",
                "status": "ok",
                "root": root.to_string_lossy(),
                "agents": agents,
            })
        }
        TodoCommand::Handoff {
            id,
            to,
            from,
            summary,
        } => handoff_task(store, id, to, from.as_deref(), summary)?,
        TodoCommand::AddOwner {
            id,
            agent,
            claim_type,
        } => add_task_owner(root, id, agent, claim_type)?,
        TodoCommand::RemoveOwner { id, agent } => remove_task_owner(root, id, agent)?,
        TodoCommand::ListOwners { id } => {
            let owners = list_task_owners(root, id)?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.list_owners",
                "status": "ok",
                "root": root.to_string_lossy(),
                "task_id": id,
                "owners": owners,
            })
        }
        TodoCommand::RegisterExpertise {
            agent,
            category,
            level,
        } => {
            let default_agent =
                env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let agent_id = agent.as_deref().unwrap_or(&default_agent);
            register_agent_expertise(root, agent_id, category, level)?
        }
        TodoCommand::Expertise { agent, category } => {
            let expertise = list_agent_expertise(root, agent.as_deref(), category.as_deref())?;
            serde_json::json!({
                "ts": now_iso(),
                "cmd": "todo.expertise",
                "status": "ok",
                "root": root.to_string_lossy(),
                "expertise": expertise,
            })
        }
    };

    match cli.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        OutputFormat::Text => match &cli.command {
            TodoCommand::List { .. } => {
                let items = out.get("items").cloned().unwrap_or(JsonValue::Null);
                if let Some(arr) = items.as_array() {
                    if arr.is_empty() {
                        println!("No tasks found.");
                        return Ok(());
                    }
                    println!("Tasks (root: {}):", root.display());
                    for v in arr {
                        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
                        let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("?");
                        let prio = v.get("priority").and_then(|x| x.as_str()).unwrap_or("?");
                        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
                        let scope = v.get("scope").and_then(|x| x.as_str()).unwrap_or("root");
                        println!("- {} [{}|{}|{}] {}", id, status, prio, scope, title);
                    }
                } else {
                    println!("No tasks found.");
                }
            }
            TodoCommand::Categories => {
                if let Some(cats) = out.get("categories").and_then(|x| x.as_array()) {
                    if cats.is_empty() {
                        println!("No categories defined.");
                    } else {
                        println!("Available categories:");
                        for cat in cats {
                            let name = cat.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                            let desc = cat
                                .get("description")
                                .and_then(|x| x.as_str())
                                .unwrap_or("");
                            let keywords =
                                cat.get("keywords").and_then(|x| x.as_str()).unwrap_or("");
                            println!("  {} - {} (keywords: {})", name, desc, keywords);
                        }
                    }
                }
            }
            TodoCommand::Ownerships { .. } => {
                if let Some(claims) = out.get("claims").and_then(|x| x.as_array()) {
                    if claims.is_empty() {
                        println!("No category ownership claims.");
                    } else {
                        println!("Category ownership claims:");
                        for claim in claims {
                            let category = claim
                                .get("category")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let agent = claim
                                .get("agent_id")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let claimed_at = claim
                                .get("claimed_at")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            println!("  {} -> {} (claimed_at: {})", category, agent, claimed_at);
                        }
                    }
                }
            }
            TodoCommand::Presence { .. } => {
                if let Some(agents) = out.get("agents").and_then(|x| x.as_array()) {
                    if agents.is_empty() {
                        println!("No agent presence records.");
                    } else {
                        println!("Agent presence:");
                        let now = now_unix_secs();
                        for agent in agents {
                            let id = agent
                                .get("agent_id")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let last_seen = agent
                                .get("last_seen")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let status =
                                agent.get("status").and_then(|x| x.as_str()).unwrap_or("?");
                            let age_secs = parse_epoch_z(last_seen)
                                .map(|v| now.saturating_sub(v).to_string())
                                .unwrap_or_else(|| "?".to_string());
                            println!(
                                "  {} (status: {}, last_seen: {}, age_s: {})",
                                id, status, last_seen, age_secs
                            );
                        }
                    }
                }
            }
            TodoCommand::ListOwners { .. } => {
                if let Some(owners) = out.get("owners").and_then(|x| x.as_array()) {
                    if owners.is_empty() {
                        println!("No additional owners for this task.");
                    } else {
                        println!("Task owners:");
                        for owner in owners {
                            let agent_id = owner
                                .get("agent_id")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let claim_type = owner
                                .get("claim_type")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            let claimed_at = owner
                                .get("claimed_at")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            println!("  {} [{}] (since: {})", agent_id, claim_type, claimed_at);
                        }
                    }
                }
            }
            TodoCommand::Expertise { .. } => {
                if let Some(expertise) = out.get("expertise").and_then(|x| x.as_array()) {
                    if expertise.is_empty() {
                        println!("No expertise records found.");
                    } else {
                        println!("Agent expertise:");
                        for exp in expertise {
                            let agent = exp.get("agent_id").and_then(|x| x.as_str()).unwrap_or("?");
                            let category =
                                exp.get("category").and_then(|x| x.as_str()).unwrap_or("?");
                            let level = exp
                                .get("expertise_level")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?");
                            println!("  {} -> {} [{}]", agent, category, level);
                        }
                    }
                }
            }
            _ => {
                // For non-list commands, text mode prints the minimal envelope.
                println!("{}", serde_json::to_string(&out).unwrap());
            }
        },
    }

    Ok(())
}
