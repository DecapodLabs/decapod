use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use ulid::Ulid;

// --- Constants ---

const VALID_NODE_TYPES: &[&str] = &[
    "decision",
    "commitment",
    "person",
    "preference",
    "lesson",
    "project",
    "handoff",
    "observation",
];

const CRITICAL_NODE_TYPES: &[&str] = &["decision", "commitment"];

const VALID_STATUSES: &[&str] = &["active", "superseded", "deprecated", "disputed"];

const VALID_PRIORITIES: &[&str] = &["critical", "notable", "background"];

const VALID_CONFIDENCES: &[&str] = &["human_confirmed", "agent_inferred", "imported"];

const VALID_EDGE_TYPES: &[&str] = &["relates_to", "depends_on", "supersedes", "invalidated_by"];

// --- CLI ---

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Parser, Debug)]
#[clap(
    name = "federation",
    about = "Governed agent memory — typed knowledge graph with provenance and lifecycle."
)]
pub struct FederationCli {
    #[clap(long, global = true, value_enum, default_value = "text")]
    pub format: OutputFormat,
    #[clap(subcommand)]
    pub command: FederationCommand,
}

#[derive(Subcommand, Debug)]
pub enum FederationCommand {
    /// Add a new memory node.
    Add {
        /// Node title
        #[clap(long)]
        title: String,
        /// Node type: decision, commitment, person, preference, lesson, project, handoff, observation
        #[clap(long = "type")]
        node_type: String,
        /// Priority: critical, notable, background
        #[clap(long, default_value = "notable")]
        priority: String,
        /// Confidence: human_confirmed, agent_inferred, imported
        #[clap(long, default_value = "agent_inferred")]
        confidence: String,
        /// Markdown body content
        #[clap(long, default_value = "")]
        body: String,
        /// Comma-separated provenance sources (scheme-prefixed: file:, url:, cmd:, commit:, event:)
        #[clap(long, default_value = "")]
        sources: String,
        /// Comma-separated tags
        #[clap(long, default_value = "")]
        tags: String,
        /// Scope: repo, user
        #[clap(long, default_value = "repo")]
        scope: String,
        /// When this claim became valid (ISO 8601)
        #[clap(long)]
        effective_from: Option<String>,
        /// Actor (defaults to 'decapod')
        #[clap(long, default_value = "decapod")]
        actor: String,
    },
    /// Get a node by ID (with sources and edges).
    Get {
        #[clap(long)]
        id: String,
    },
    /// List nodes with filters.
    List {
        #[clap(long = "type")]
        node_type: Option<String>,
        #[clap(long)]
        status: Option<String>,
        #[clap(long)]
        priority: Option<String>,
        #[clap(long)]
        scope: Option<String>,
    },
    /// Search nodes by title and body text.
    Search {
        #[clap(long)]
        query: String,
        #[clap(long)]
        scope: Option<String>,
    },
    /// Edit a non-critical node's fields.
    Edit {
        #[clap(long)]
        id: String,
        #[clap(long)]
        title: Option<String>,
        #[clap(long)]
        body: Option<String>,
        #[clap(long)]
        tags: Option<String>,
        #[clap(long)]
        priority: Option<String>,
    },
    /// Supersede a node: transitions old to 'superseded' and creates a supersedes edge.
    Supersede {
        /// The node being superseded
        #[clap(long)]
        id: String,
        /// The new node that supersedes it
        #[clap(long)]
        by: String,
        /// Reason for supersession
        #[clap(long, default_value = "")]
        reason: String,
    },
    /// Mark a node as deprecated.
    Deprecate {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "")]
        reason: String,
    },
    /// Mark a node as disputed.
    Dispute {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "")]
        reason: String,
    },
    /// Add a typed edge between nodes.
    Link {
        /// Source node ID
        #[clap(long)]
        source: String,
        /// Target node ID
        #[clap(long)]
        target: String,
        /// Edge type: relates_to, depends_on, supersedes, invalidated_by
        #[clap(long = "type")]
        edge_type: String,
    },
    /// Remove an edge by ID.
    Unlink {
        #[clap(long)]
        id: String,
    },
    /// Show node neighborhood (graph traversal).
    Graph {
        #[clap(long)]
        id: String,
        /// Traversal depth
        #[clap(long, default_value = "1")]
        depth: u32,
    },
    /// Add a provenance source to an existing node.
    SourcesAdd {
        /// Node ID to add source to
        #[clap(long)]
        id: String,
        /// Provenance source (scheme-prefixed: file:, url:, cmd:, commit:, event:)
        #[clap(long)]
        source: String,
    },
    /// Initialize federation DB and events file (no-op if already initialized).
    Init,
    /// Export Obsidian-compatible vault notes under federation/vault/.
    VaultExport,
    /// Build deterministic index file at federation/_index.md.
    IndexBuild,
    /// Export deterministic graph file at federation/_graph.json.
    GraphExport,
    /// Rebuild federation.db deterministically from federation.events.jsonl.
    Rebuild,
    /// Print the JSON schema for the federation subsystem.
    Schema,
}

// --- Data Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FederationNode {
    pub id: String,
    pub node_type: String,
    pub status: String,
    pub priority: String,
    pub confidence: String,
    pub title: String,
    pub body: String,
    pub scope: String,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
    pub effective_from: Option<String>,
    pub effective_to: Option<String>,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<FederationEdge>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FederationEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub created_at: String,
    pub actor: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FederationEvent {
    event_id: String,
    ts: String,
    event_type: String,
    #[serde(default = "default_federation_event_status")]
    status: String,
    node_id: Option<String>,
    payload: JsonValue,
    actor: String,
}

fn default_federation_event_status() -> String {
    "success".to_string()
}

// --- Helpers ---

fn now_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

pub fn federation_db_path(root: &Path) -> PathBuf {
    root.join(schemas::MEMORY_DB_NAME)
}

fn federation_events_path(root: &Path) -> PathBuf {
    root.join(schemas::FEDERATION_EVENTS_NAME)
}

fn federation_derived_dir(root: &Path) -> PathBuf {
    root.join("federation")
}

fn federation_vault_dir(root: &Path) -> PathBuf {
    federation_derived_dir(root).join("vault")
}

fn federation_index_path(root: &Path) -> PathBuf {
    federation_derived_dir(root).join("_index.md")
}

fn federation_graph_path(root: &Path) -> PathBuf {
    federation_derived_dir(root).join("_graph.json")
}

fn validate_node_type(t: &str) -> Result<(), error::DecapodError> {
    if !VALID_NODE_TYPES.contains(&t) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid node_type '{}'. Must be one of: {}",
            t,
            VALID_NODE_TYPES.join(", ")
        )));
    }
    Ok(())
}

fn validate_status(s: &str) -> Result<(), error::DecapodError> {
    if !VALID_STATUSES.contains(&s) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid status '{}'. Must be one of: {}",
            s,
            VALID_STATUSES.join(", ")
        )));
    }
    Ok(())
}

fn validate_priority(p: &str) -> Result<(), error::DecapodError> {
    if !VALID_PRIORITIES.contains(&p) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid priority '{}'. Must be one of: {}",
            p,
            VALID_PRIORITIES.join(", ")
        )));
    }
    Ok(())
}

fn validate_confidence(c: &str) -> Result<(), error::DecapodError> {
    if !VALID_CONFIDENCES.contains(&c) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid confidence '{}'. Must be one of: {}",
            c,
            VALID_CONFIDENCES.join(", ")
        )));
    }
    Ok(())
}

fn validate_edge_type(t: &str) -> Result<(), error::DecapodError> {
    if !VALID_EDGE_TYPES.contains(&t) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid edge_type '{}'. Must be one of: {}",
            t,
            VALID_EDGE_TYPES.join(", ")
        )));
    }
    Ok(())
}

fn validate_provenance(source: &str) -> Result<(), error::DecapodError> {
    let prov_re = regex::Regex::new(
        r"^(file:[^#]+(#L\d+(-L\d+)?)?|url:[^ ]+|cmd:[^ ]+|commit:[a-f0-9]+|event:.+)$",
    )
    .unwrap();

    if !prov_re.is_match(source) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid provenance source: '{}'. Must match scheme (file:|url:|cmd:|commit:|event:)",
            source
        )));
    }
    Ok(())
}

fn is_critical(node_type: &str, priority: &str) -> bool {
    CRITICAL_NODE_TYPES.contains(&node_type) || priority == "critical"
}

fn append_event(events_path: &Path, event: &FederationEvent) -> Result<(), error::DecapodError> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(events_path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(event).unwrap())
        .map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn node_exists(conn: &Connection, id: &str) -> Result<bool, error::DecapodError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub fn find_node_by_source(
    store: &Store,
    source_pattern: &str,
) -> Result<Option<String>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);

    broker.with_conn(
        &db_path,
        "decapod",
        None,
        "federation.find_by_source",
        |conn| {
            let node_id: Option<String> = conn
                .query_row(
                    "SELECT n.id FROM nodes n 
             JOIN sources s ON n.id = s.node_id 
             WHERE s.source = ?1 
             LIMIT 1",
                    params![source_pattern],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(node_id)
        },
    )
}

fn get_node_type_and_priority(
    conn: &Connection,
    id: &str,
) -> Result<(String, String), error::DecapodError> {
    conn.query_row(
        "SELECT node_type, priority FROM nodes WHERE id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|_| error::DecapodError::NotFound(format!("Node '{}' not found", id)))
}

fn get_node_status(conn: &Connection, id: &str) -> Result<String, error::DecapodError> {
    conn.query_row(
        "SELECT status FROM nodes WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .map_err(|_| error::DecapodError::NotFound(format!("Node '{}' not found", id)))
}

fn read_node_full(conn: &Connection, id: &str) -> Result<FederationNode, error::DecapodError> {
    let node = conn
        .query_row(
            "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                    created_at, updated_at, effective_from, effective_to, actor
             FROM nodes WHERE id = ?1",
            params![id],
            |row| {
                Ok(FederationNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    status: row.get(2)?,
                    priority: row.get(3)?,
                    confidence: row.get(4)?,
                    title: row.get(5)?,
                    body: row.get(6)?,
                    scope: row.get(7)?,
                    tags: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    effective_from: row.get(11)?,
                    effective_to: row.get(12)?,
                    actor: row.get(13)?,
                    sources: None,
                    edges: None,
                })
            },
        )
        .map_err(|_| error::DecapodError::NotFound(format!("Node '{}' not found", id)))?;

    // Fetch sources
    let mut stmt = conn.prepare("SELECT source FROM sources WHERE node_id = ?1")?;
    let sources: Vec<String> = stmt
        .query_map(params![id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Fetch edges (both directions)
    let mut edge_stmt = conn.prepare(
        "SELECT id, source_id, target_id, edge_type, created_at, actor
             FROM edges WHERE source_id = ?1 OR target_id = ?1",
    )?;
    let edges: Vec<FederationEdge> = edge_stmt
        .query_map(params![id], |row| {
            Ok(FederationEdge {
                id: row.get(0)?,
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                edge_type: row.get(3)?,
                created_at: row.get(4)?,
                actor: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(FederationNode {
        sources: Some(sources),
        edges: Some(edges),
        ..node
    })
}

// --- Initialization ---

pub fn initialize_federation_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = federation_db_path(root);
    let broker = DbBroker::new(root);
    broker.with_conn(&db_path, "decapod", None, "federation.init", |conn| {
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_META)?;
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_NODES)?;
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_SOURCES)?;
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EDGES)?;
        conn.execute_batch(schemas::MEMORY_DB_SCHEMA_EVENTS)?;

        // Indexes
        conn.execute_batch(schemas::MEMORY_DB_INDEX_NODES_TYPE)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_NODES_STATUS)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_NODES_SCOPE)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_NODES_PRIORITY)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_NODES_UPDATED)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_SOURCES_NODE)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_EDGES_SOURCE)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_EDGES_TARGET)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_EDGES_TYPE)?;
        conn.execute_batch(schemas::MEMORY_DB_INDEX_EVENTS_NODE)?;

        // Version tracking
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', ?1)",
            params![schemas::MEMORY_SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    })?;

    // Create events file if missing
    let events_path = federation_events_path(root);
    if !events_path.exists() {
        fs::write(&events_path, "").map_err(error::DecapodError::IoError)?;
    }

    Ok(())
}

// --- Core Operations ---

#[allow(clippy::too_many_arguments)]
pub fn add_node(
    store: &Store,
    title: &str,
    node_type: &str,
    priority: &str,
    confidence: &str,
    body: &str,
    sources_str: &str,
    tags: &str,
    scope: &str,
    effective_from: Option<&str>,
    actor: &str,
) -> Result<FederationNode, error::DecapodError> {
    validate_node_type(node_type)?;
    validate_priority(priority)?;
    validate_confidence(confidence)?;

    // Parse and validate sources
    let sources: Vec<String> = if sources_str.is_empty() {
        vec![]
    } else {
        sources_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    for src in &sources {
        validate_provenance(src)?;
    }

    // Enforce provenance for critical types
    if is_critical(node_type, priority) && sources.is_empty() {
        return Err(error::DecapodError::ValidationError(format!(
            "Provenance required: node_type='{}' with priority='{}' requires at least one source (file:|url:|cmd:|commit:|event:)",
            node_type, priority
        )));
    }

    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();
    let node_id = format!("F_{}", Ulid::new());
    let event_id = Ulid::new().to_string();

    let payload_json = serde_json::json!({
        "title": title,
        "node_type": node_type,
        "priority": priority,
        "confidence": confidence,
        "body": body,
        "sources": sources,
        "tags": tags,
        "scope": scope,
        "effective_from": effective_from,
        "dir_path": store.root.to_string_lossy().to_string(),
    });

    let node = broker.with_conn(&db_path, actor, None, "federation.add", |conn| {
        conn.execute(
            "INSERT INTO nodes(id, node_type, status, priority, confidence, title, body, scope, tags, created_at, updated_at, effective_from, dir_path, actor)
             VALUES(?1, ?2, 'active', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                node_id, node_type, priority, confidence, title, body, scope, tags,
                now, now, effective_from, store.root.to_string_lossy().to_string(), actor
            ],
        )?;

        // Insert sources
        for src in &sources {
            let src_id = format!("FS_{}", Ulid::new());
            conn.execute(
                "INSERT INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)",
                params![src_id, node_id, src, now],
            )?;
        }

        // Insert event record
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event_id,
                now,
                "node.create",
                node_id,
                serde_json::to_string(&payload_json).unwrap(),
                actor,
            ],
        )?;

        // Append to JSONL inside the same logical unit to prevent drift
        append_event(
            &events_path,
            &FederationEvent {
                event_id: event_id.clone(),
                ts: now.clone(),
                event_type: "node.create".to_string(),
                status: "success".to_string(),
                node_id: Some(node_id.clone()),
                payload: payload_json.clone(),
                actor: actor.to_string(),
            },
        )?;

        Ok(FederationNode {
            id: node_id.clone(),
            node_type: node_type.to_string(),
            status: "active".to_string(),
            priority: priority.to_string(),
            confidence: confidence.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            scope: scope.to_string(),
            tags: tags.to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            effective_from: effective_from.map(|s| s.to_string()),
            effective_to: None,
            actor: actor.to_string(),
            sources: Some(sources.clone()),
            edges: Some(vec![]),
        })
    })?;

    Ok(node)
}

pub fn edit_node(
    store: &Store,
    id: &str,
    title: Option<&str>,
    body: Option<&str>,
    tags: Option<&str>,
    priority: Option<&str>,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);

    if let Some(p) = priority {
        validate_priority(p)?;
    }

    let now = now_ts();

    broker.with_conn(&db_path, "decapod", None, "federation.edit", |conn| {
        // Check node exists and is not critical
        let (nt, pri) = get_node_type_and_priority(conn, id)?;
        if is_critical(&nt, &pri) {
            return Err(error::DecapodError::ValidationError(format!(
                "Cannot edit critical node '{}' (type={}, priority={}). Use 'supersede' instead.",
                id, nt, pri
            )));
        }

        let status = get_node_status(conn, id)?;
        if status != "active" {
            return Err(error::DecapodError::ValidationError(format!(
                "Cannot edit node '{}' with status '{}'. Only active nodes can be edited.",
                id, status
            )));
        }

        // Build dynamic update
        let mut sets = vec!["updated_at = ?1".to_string()];
        let mut param_idx = 2u32;
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now.clone())];

        if let Some(t) = title {
            sets.push(format!("title = ?{}", param_idx));
            param_values.push(Box::new(t.to_string()));
            param_idx += 1;
        }
        if let Some(b) = body {
            sets.push(format!("body = ?{}", param_idx));
            param_values.push(Box::new(b.to_string()));
            param_idx += 1;
        }
        if let Some(tg) = tags {
            sets.push(format!("tags = ?{}", param_idx));
            param_values.push(Box::new(tg.to_string()));
            param_idx += 1;
        }
        if let Some(p) = priority {
            // Don't allow escalating to critical on edit
            if p == "critical" {
                return Err(error::DecapodError::ValidationError(
                    "Cannot escalate to 'critical' priority via edit. Create a new critical node instead.".to_string(),
                ));
            }
            sets.push(format!("priority = ?{}", param_idx));
            param_values.push(Box::new(p.to_string()));
            param_idx += 1;
        }
        let _ = param_idx; // suppress unused warning

        let sql = format!("UPDATE nodes SET {} WHERE id = ?{}", sets.join(", "), param_values.len() + 1);
        param_values.push(Box::new(id.to_string()));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;

        // Record event in DB
        let event_id = Ulid::new().to_string();
        let payload_json = serde_json::json!({
            "title": title,
            "body": body,
            "tags": tags,
            "priority": priority,
        });
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event_id,
                now,
                "node.edit",
                id,
                serde_json::to_string(&payload_json).unwrap(),
                "decapod",
            ],
        )?;

        append_event(
            &events_path,
            &FederationEvent {
                event_id,
                ts: now.clone(),
                event_type: "node.edit".to_string(),
                status: "success".to_string(),
                node_id: Some(id.to_string()),
                payload: payload_json,
                actor: "decapod".to_string(),
            },
        )?;

        Ok(())
    })?;

    Ok(())
}

pub fn supersede_node(
    store: &Store,
    old_id: &str,
    new_id: &str,
    reason: &str,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();

    broker.with_conn(&db_path, "decapod", None, "federation.supersede", |conn| {
        // Verify both nodes exist
        if !node_exists(conn, old_id)? {
            return Err(error::DecapodError::NotFound(format!(
                "Node '{}' not found",
                old_id
            )));
        }
        if !node_exists(conn, new_id)? {
            return Err(error::DecapodError::NotFound(format!(
                "Node '{}' not found",
                new_id
            )));
        }

        // Old node must be active
        let old_status = get_node_status(conn, old_id)?;
        if old_status != "active" {
            return Err(error::DecapodError::ValidationError(format!(
                "Cannot supersede node '{}' with status '{}'. Only active nodes can be superseded.",
                old_id, old_status
            )));
        }

        // Transition old node
        conn.execute(
            "UPDATE nodes SET status = 'superseded', updated_at = ?1, effective_to = ?1 WHERE id = ?2",
            params![now, old_id],
        )?;

        // Create supersedes edge
        let edge_id = format!("FE_{}", Ulid::new());
        conn.execute(
            "INSERT INTO edges(id, source_id, target_id, edge_type, created_at, actor)
             VALUES(?1, ?2, ?3, 'supersedes', ?4, 'decapod')",
            params![edge_id, new_id, old_id, now],
        )?;

        // Record event
        let event_id = Ulid::new().to_string();
        let payload_json = serde_json::json!({
            "old_id": old_id,
            "new_id": new_id,
            "reason": reason,
            "edge_id": edge_id,
        });
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event_id,
                now,
                "node.supersede",
                old_id,
                serde_json::to_string(&payload_json).unwrap(),
                "decapod",
            ],
        )?;

        append_event(
            &events_path,
            &FederationEvent {
                event_id,
                ts: now.clone(),
                event_type: "node.supersede".to_string(),
                status: "success".to_string(),
                node_id: Some(old_id.to_string()),
                payload: payload_json,
                actor: "decapod".to_string(),
            },
        )?;

        Ok(())
    })?;

    Ok(())
}

pub fn transition_node_status(
    store: &Store,
    id: &str,
    new_status: &str,
    event_type: &str,
    reason: &str,
) -> Result<(), error::DecapodError> {
    validate_status(new_status)?;

    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();

    broker.with_conn(&db_path, "decapod", None, &format!("federation.{}", event_type), |conn| {
        let old_status = get_node_status(conn, id)?;
        if old_status != "active" {
            return Err(error::DecapodError::ValidationError(format!(
                "Cannot transition node '{}' from '{}' to '{}'. Only active nodes can be transitioned.",
                id, old_status, new_status
            )));
        }

        conn.execute(
            "UPDATE nodes SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_status, now, id],
        )?;

        let event_id = Ulid::new().to_string();
        let payload_json = serde_json::json!({
            "new_status": new_status,
            "reason": reason,
        });
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event_id,
                now,
                event_type,
                id,
                serde_json::to_string(&payload_json).unwrap(),
                "decapod",
            ],
        )?;

        append_event(
            &events_path,
            &FederationEvent {
                event_id,
                ts: now.clone(),
                event_type: event_type.to_string(),
                status: "success".to_string(),
                node_id: Some(id.to_string()),
                payload: payload_json,
                actor: "decapod".to_string(),
            },
        )?;

        Ok(())
    })?;

    Ok(())
}

pub fn add_edge(
    store: &Store,
    source_id: &str,
    target_id: &str,
    edge_type: &str,
) -> Result<String, error::DecapodError> {
    validate_edge_type(edge_type)?;

    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();
    let edge_id = format!("FE_{}", Ulid::new());

    broker.with_conn(&db_path, "decapod", None, "federation.link", |conn| {
        if !node_exists(conn, source_id)? {
            return Err(error::DecapodError::NotFound(format!(
                "Source node '{}' not found",
                source_id
            )));
        }
        if !node_exists(conn, target_id)? {
            return Err(error::DecapodError::NotFound(format!(
                "Target node '{}' not found",
                target_id
            )));
        }

        conn.execute(
            "INSERT INTO edges(id, source_id, target_id, edge_type, created_at, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, 'decapod')",
            params![edge_id, source_id, target_id, edge_type, now],
        )?;

        let event_id = Ulid::new().to_string();
        let payload_json = serde_json::json!({
            "edge_id": edge_id,
            "source_id": source_id,
            "target_id": target_id,
            "edge_type": edge_type,
        });
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event_id,
                now,
                "edge.add",
                source_id,
                serde_json::to_string(&payload_json).unwrap(),
                "decapod",
            ],
        )?;

        append_event(
            &events_path,
            &FederationEvent {
                event_id,
                ts: now.clone(),
                event_type: "edge.add".to_string(),
                status: "success".to_string(),
                node_id: Some(source_id.to_string()),
                payload: payload_json,
                actor: "decapod".to_string(),
            },
        )?;

        Ok(())
    })?;

    Ok(edge_id.clone())
}

fn remove_edge(store: &Store, edge_id: &str) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();

    broker.with_conn(&db_path, "decapod", None, "federation.unlink", |conn| {
        let changes = conn.execute("DELETE FROM edges WHERE id = ?1", params![edge_id])?;

        if changes == 0 {
            return Err(error::DecapodError::NotFound(format!(
                "Edge '{}' not found",
                edge_id
            )));
        }

        let event_id = Ulid::new().to_string();
        let payload_json = serde_json::json!({ "edge_id": edge_id });
        conn.execute(
            "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, NULL, ?4, ?5)",
            params![
                event_id,
                now,
                "edge.remove",
                serde_json::to_string(&payload_json).unwrap(),
                "decapod",
            ],
        )?;

        append_event(
            &events_path,
            &FederationEvent {
                event_id,
                ts: now.clone(),
                event_type: "edge.remove".to_string(),
                status: "success".to_string(),
                node_id: None,
                payload: payload_json,
                actor: "decapod".to_string(),
            },
        )?;

        Ok(())
    })?;

    Ok(())
}

pub fn add_source_to_node(
    store: &Store,
    node_id: &str,
    source: &str,
) -> Result<String, error::DecapodError> {
    validate_provenance(source)?;

    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let events_path = federation_events_path(&store.root);
    let now = now_ts();
    let src_id = format!("FS_{}", Ulid::new());

    broker.with_conn(
        &db_path,
        "decapod",
        None,
        "federation.sources.add",
        |conn| {
            if !node_exists(conn, node_id)? {
                return Err(error::DecapodError::NotFound(format!(
                    "Node '{}' not found",
                    node_id
                )));
            }

            conn.execute(
                "INSERT INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)",
                params![src_id, node_id, source, now],
            )?;

            // Update node timestamp
            conn.execute(
                "UPDATE nodes SET updated_at = ?1 WHERE id = ?2",
                params![now, node_id],
            )?;

            let event_id = Ulid::new().to_string();
            let payload_json = serde_json::json!({
                "source_id": src_id,
                "source": source,
            });
            conn.execute(
                "INSERT INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event_id,
                    now,
                    "source.add",
                    node_id,
                    serde_json::to_string(&payload_json).unwrap(),
                    "decapod",
                ],
            )?;

            append_event(
                &events_path,
                &FederationEvent {
                    event_id,
                    ts: now.clone(),
                    event_type: "source.add".to_string(),
                    status: "success".to_string(),
                    node_id: Some(node_id.to_string()),
                    payload: payload_json,
                    actor: "decapod".to_string(),
                },
            )?;

            Ok(())
        },
    )?;

    Ok(src_id.clone())
}

fn graph_neighbors(store: &Store, id: &str, depth: u32) -> Result<JsonValue, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "federation.graph", |conn| {
        if !node_exists(conn, id)? {
            return Err(error::DecapodError::NotFound(format!(
                "Node '{}' not found",
                id
            )));
        }

        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut frontier = vec![id.to_string()];
        let mut all_nodes = vec![];
        let mut all_edges = vec![];

        for _d in 0..depth {
            let mut next_frontier = vec![];
            for node_id in &frontier {
                if visited.contains(node_id) {
                    continue;
                }
                visited.insert(node_id.clone());

                let node = read_node_full(conn, node_id)?;
                if let Some(ref edges) = node.edges {
                    for edge in edges {
                        all_edges.push(serde_json::json!({
                            "id": edge.id,
                            "source_id": edge.source_id,
                            "target_id": edge.target_id,
                            "edge_type": edge.edge_type,
                        }));
                        let neighbor = if edge.source_id == *node_id {
                            &edge.target_id
                        } else {
                            &edge.source_id
                        };
                        if !visited.contains(neighbor) {
                            next_frontier.push(neighbor.clone());
                        }
                    }
                }
                all_nodes.push(serde_json::json!({
                    "id": node.id,
                    "title": node.title,
                    "node_type": node.node_type,
                    "status": node.status,
                    "priority": node.priority,
                }));
            }
            frontier = next_frontier;
        }

        // Also load the final frontier nodes (without their edges)
        for node_id in &frontier {
            if !visited.contains(node_id) {
                visited.insert(node_id.clone());
                if let Ok(node) = read_node_full(conn, node_id) {
                    all_nodes.push(serde_json::json!({
                        "id": node.id,
                        "title": node.title,
                        "node_type": node.node_type,
                        "status": node.status,
                        "priority": node.priority,
                    }));
                }
            }
        }

        Ok(serde_json::json!({
            "root": id,
            "depth": depth,
            "nodes": all_nodes,
            "edges": all_edges,
        }))
    })
}

fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn yaml_escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_index_markdown(conn: &Connection) -> Result<String, error::DecapodError> {
    let mut out = String::new();
    out.push_str("# Federation Vault Index\n\n");
    out.push_str("| Note | Description |\n");
    out.push_str("|------|-------------|\n");

    let mut stmt = conn.prepare(
        "SELECT id, node_type, title, status, priority
         FROM nodes
         ORDER BY node_type, id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let node_type: String = row.get(1)?;
        let title: String = row.get(2)?;
        let status: String = row.get(3)?;
        let priority: String = row.get(4)?;
        let note = format!("vault/{}/{}.md", node_type, id);
        let desc = format!("{} [{}|{}]", title.replace('|', "\\|"), status, priority);
        out.push_str(&format!("| {} | {} |\n", note, desc));
    }

    Ok(out)
}

fn build_graph_json(conn: &Connection) -> Result<JsonValue, error::DecapodError> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    {
        let mut stmt = conn.prepare(
            "SELECT id, node_type, status, priority, title
             FROM nodes ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            nodes.push(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "node_type": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "priority": row.get::<_, String>(3)?,
                "title": row.get::<_, String>(4)?,
            }));
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, source_id, target_id, edge_type
             FROM edges ORDER BY source_id, edge_type, target_id, id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            edges.push(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "source_id": row.get::<_, String>(1)?,
                "target_id": row.get::<_, String>(2)?,
                "edge_type": row.get::<_, String>(3)?,
            }));
        }
    }

    Ok(serde_json::json!({
        "version": "1",
        "nodes": nodes,
        "edges": edges
    }))
}

fn export_vault_notes(store: &Store) -> Result<usize, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let vault_dir = federation_vault_dir(&store.root);
    fs::create_dir_all(&vault_dir).map_err(error::DecapodError::IoError)?;

    broker.with_conn(&db_path, "decapod", None, "federation.vault.export", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                    created_at, updated_at, effective_from, effective_to, actor
             FROM nodes ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FederationNode {
                id: row.get(0)?,
                node_type: row.get(1)?,
                status: row.get(2)?,
                priority: row.get(3)?,
                confidence: row.get(4)?,
                title: row.get(5)?,
                body: row.get(6)?,
                scope: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                effective_from: row.get(11)?,
                effective_to: row.get(12)?,
                actor: row.get(13)?,
                sources: None,
                edges: None,
            })
        })?;

        let mut count = 0usize;
        for row in rows {
            let mut node = row?;

            let mut src_stmt =
                conn.prepare("SELECT source FROM sources WHERE node_id = ?1 ORDER BY source")?;
            let sources = src_stmt
                .query_map(params![node.id.clone()], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            node.sources = Some(sources);

            let mut edge_stmt = conn.prepare(
                "SELECT edge_type, target_id FROM edges WHERE source_id = ?1 ORDER BY edge_type, target_id",
            )?;
            let outgoing = edge_stmt
                .query_map(params![node.id.clone()], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<(String, String)>, _>>()?;

            let node_type_dir = vault_dir.join(&node.node_type);
            fs::create_dir_all(&node_type_dir).map_err(error::DecapodError::IoError)?;
            let note_path = node_type_dir.join(format!("{}.md", node.id));

            let tags = parse_tags(&node.tags);
            let tags_yaml = if tags.is_empty() {
                "[]".to_string()
            } else {
                format!(
                    "[{}]",
                    tags.iter()
                        .map(|t| format!("\"{}\"", yaml_escape(t)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };

            let mut md = String::new();
            md.push_str("---\n");
            md.push_str(&format!("id: \"{}\"\n", yaml_escape(&node.id)));
            md.push_str(&format!("type: \"{}\"\n", yaml_escape(&node.node_type)));
            md.push_str(&format!("status: \"{}\"\n", yaml_escape(&node.status)));
            md.push_str(&format!("priority: \"{}\"\n", yaml_escape(&node.priority)));
            md.push_str(&format!("confidence: \"{}\"\n", yaml_escape(&node.confidence)));
            md.push_str(&format!("title: \"{}\"\n", yaml_escape(&node.title)));
            md.push_str(&format!("scope: \"{}\"\n", yaml_escape(&node.scope)));
            md.push_str(&format!("tags: {}\n", tags_yaml));
            md.push_str(&format!("created_at: \"{}\"\n", yaml_escape(&node.created_at)));
            md.push_str(&format!("updated_at: \"{}\"\n", yaml_escape(&node.updated_at)));
            match node.effective_from.as_ref() {
                Some(v) => md.push_str(&format!("effective_from: \"{}\"\n", yaml_escape(v))),
                None => md.push_str("effective_from: null\n"),
            }
            match node.effective_to.as_ref() {
                Some(v) => md.push_str(&format!("effective_to: \"{}\"\n", yaml_escape(v))),
                None => md.push_str("effective_to: null\n"),
            }
            md.push_str(&format!("actor: \"{}\"\n", yaml_escape(&node.actor)));
            md.push_str("sources:\n");
            if let Some(ref srcs) = node.sources {
                if srcs.is_empty() {
                    md.push_str("  []\n");
                } else {
                    for src in srcs {
                        md.push_str(&format!("  - \"{}\"\n", yaml_escape(src)));
                    }
                }
            } else {
                md.push_str("  []\n");
            }
            md.push_str("edges:\n");
            if outgoing.is_empty() {
                md.push_str("  []\n");
            } else {
                for (edge_type, target_id) in outgoing {
                    md.push_str(&format!(
                        "  - {{ type: \"{}\", target: \"{}\" }}\n",
                        yaml_escape(&edge_type),
                        yaml_escape(&target_id)
                    ));
                }
            }
            md.push_str("---\n\n");
            md.push_str(
                "<!-- Derived artifact. Edit through `decapod data federation` commands. -->\n\n",
            );
            md.push_str(&node.body);
            md.push('\n');

            fs::write(&note_path, md).map_err(error::DecapodError::IoError)?;
            count += 1;
        }

        Ok(count)
    })
}

fn build_index_file(store: &Store) -> Result<usize, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let derived_dir = federation_derived_dir(&store.root);
    fs::create_dir_all(&derived_dir).map_err(error::DecapodError::IoError)?;
    let path = federation_index_path(&store.root);

    let content = broker.with_conn(
        &db_path,
        "decapod",
        None,
        "federation.index.build",
        build_index_markdown,
    )?;
    fs::write(path, content.as_bytes()).map_err(error::DecapodError::IoError)?;
    Ok(content.lines().count())
}

pub fn refresh_derived_files(store: &Store) -> Result<(), error::DecapodError> {
    build_index_file(store)?;
    export_graph_file(store)?;
    Ok(())
}

fn export_graph_file(store: &Store) -> Result<(usize, usize), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = federation_db_path(&store.root);
    let derived_dir = federation_derived_dir(&store.root);
    fs::create_dir_all(&derived_dir).map_err(error::DecapodError::IoError)?;
    let path = federation_graph_path(&store.root);

    let graph = broker.with_conn(
        &db_path,
        "decapod",
        None,
        "federation.graph.export",
        build_graph_json,
    )?;
    let nodes = graph
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let edges = graph
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    fs::write(path, serde_json::to_string_pretty(&graph).unwrap())
        .map_err(error::DecapodError::IoError)?;
    Ok((nodes, edges))
}

// --- Rebuild ---

pub fn rebuild_from_events(root: &Path) -> Result<usize, error::DecapodError> {
    let events_path = federation_events_path(root);
    if !events_path.exists() {
        // No events file — initialize empty DB
        initialize_federation_db(root)?;
        return Ok(0);
    }

    // Create temp DB, replay events, swap
    let tmp_db = root.join(".federation.db.tmp");
    if tmp_db.exists() {
        fs::remove_file(&tmp_db).map_err(error::DecapodError::IoError)?;
    }

    let conn = crate::core::db::db_connect(&tmp_db.to_string_lossy())?;

    // Initialize schema
    conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_META)?;
    conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_NODES)?;
    conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_SOURCES)?;
    conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_EDGES)?;
    conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_EVENTS)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_NODES_TYPE)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_NODES_STATUS)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_NODES_SCOPE)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_NODES_PRIORITY)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_NODES_UPDATED)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_SOURCES_NODE)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_EDGES_SOURCE)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_EDGES_TARGET)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_EDGES_TYPE)?;
    conn.execute_batch(schemas::FEDERATION_DB_INDEX_EVENTS_NODE)?;

    conn.execute(
        "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', ?1)",
        params![schemas::FEDERATION_SCHEMA_VERSION.to_string()],
    )?;

    let file = fs::File::open(&events_path).map_err(error::DecapodError::IoError)?;
    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line.map_err(error::DecapodError::IoError)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let event: FederationEvent = serde_json::from_str(line).map_err(|e| {
            error::DecapodError::ValidationError(format!("Invalid event JSON: {}", e))
        })?;

        // Skip incomplete pending events (crash recovery)
        if event.status == "pending" {
            continue;
        }

        replay_event(&conn, &event)?;
        count += 1;
    }

    // Close connection before rename
    drop(conn);

    let db_path = federation_db_path(root);
    fs::rename(&tmp_db, &db_path).map_err(error::DecapodError::IoError)?;

    Ok(count)
}

fn replay_event(conn: &Connection, event: &FederationEvent) -> Result<(), error::DecapodError> {
    // Always record the event in the DB events table
    conn.execute(
        "INSERT OR IGNORE INTO federation_events(event_id, ts, event_type, node_id, payload, actor)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event.event_id,
            event.ts,
            event.event_type,
            event.node_id,
            serde_json::to_string(&event.payload).unwrap(),
            event.actor,
        ],
    )?;

    match event.event_type.as_str() {
        "node.create" => {
            let p = &event.payload;
            let node_id = event.node_id.as_deref().unwrap_or("");
            let dir_path = p.get("dir_path").and_then(|v| v.as_str()).unwrap_or("");

            conn.execute(
                "INSERT INTO nodes(id, node_type, status, priority, confidence, title, body, scope, tags, created_at, updated_at, effective_from, dir_path, actor)
                 VALUES(?1, ?2, 'active', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    node_id,
                    p.get("node_type").and_then(|v| v.as_str()).unwrap_or(""),
                    p.get("priority").and_then(|v| v.as_str()).unwrap_or("notable"),
                    p.get("confidence").and_then(|v| v.as_str()).unwrap_or("agent_inferred"),
                    p.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                    p.get("body").and_then(|v| v.as_str()).unwrap_or(""),
                    p.get("scope").and_then(|v| v.as_str()).unwrap_or("repo"),
                    p.get("tags").and_then(|v| v.as_str()).unwrap_or(""),
                    event.ts,
                    event.ts,
                    p.get("effective_from").and_then(|v| v.as_str()),
                    dir_path,
                    event.actor,
                ],
            )?;

            // Sources
            if let Some(sources) = p.get("sources").and_then(|v| v.as_array()) {
                for src in sources {
                    if let Some(s) = src.as_str() {
                        let src_id = format!("FS_{}", Ulid::new());
                        conn.execute(
                            "INSERT INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)",
                            params![src_id, node_id, s, event.ts],
                        )?;
                    }
                }
            }
        }
        "node.edit" => {
            let node_id = event.node_id.as_deref().unwrap_or("");
            let p = &event.payload;
            if let Some(title) = p.get("title").and_then(|v| v.as_str()) {
                conn.execute(
                    "UPDATE nodes SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, event.ts, node_id],
                )?;
            }
            if let Some(body) = p.get("body").and_then(|v| v.as_str()) {
                conn.execute(
                    "UPDATE nodes SET body = ?1, updated_at = ?2 WHERE id = ?3",
                    params![body, event.ts, node_id],
                )?;
            }
            if let Some(tags) = p.get("tags").and_then(|v| v.as_str()) {
                conn.execute(
                    "UPDATE nodes SET tags = ?1, updated_at = ?2 WHERE id = ?3",
                    params![tags, event.ts, node_id],
                )?;
            }
            if let Some(pri) = p.get("priority").and_then(|v| v.as_str()) {
                conn.execute(
                    "UPDATE nodes SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                    params![pri, event.ts, node_id],
                )?;
            }
        }
        "node.supersede" => {
            let p = &event.payload;
            let old_id = p.get("old_id").and_then(|v| v.as_str()).unwrap_or("");
            let new_id = p.get("new_id").and_then(|v| v.as_str()).unwrap_or("");

            conn.execute(
                "UPDATE nodes SET status = 'superseded', updated_at = ?1, effective_to = ?1 WHERE id = ?2",
                params![event.ts, old_id],
            )
            ?;

            let fallback_edge_id = format!("FE_{}", Ulid::new());
            let edge_id = p
                .get("edge_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&fallback_edge_id);
            conn.execute(
                "INSERT OR IGNORE INTO edges(id, source_id, target_id, edge_type, created_at, actor)
                 VALUES(?1, ?2, ?3, 'supersedes', ?4, ?5)",
                params![edge_id, new_id, old_id, event.ts, event.actor],
            )
            ?;
        }
        "node.deprecate" => {
            let node_id = event.node_id.as_deref().unwrap_or("");
            conn.execute(
                "UPDATE nodes SET status = 'deprecated', updated_at = ?1 WHERE id = ?2",
                params![event.ts, node_id],
            )?;
        }
        "node.dispute" => {
            let node_id = event.node_id.as_deref().unwrap_or("");
            conn.execute(
                "UPDATE nodes SET status = 'disputed', updated_at = ?1 WHERE id = ?2",
                params![event.ts, node_id],
            )?;
        }
        "edge.add" => {
            let p = &event.payload;
            let edge_id = p.get("edge_id").and_then(|v| v.as_str()).unwrap_or("");
            let source_id = p.get("source_id").and_then(|v| v.as_str()).unwrap_or("");
            let target_id = p.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
            let edge_type = p.get("edge_type").and_then(|v| v.as_str()).unwrap_or("");

            conn.execute(
                "INSERT OR IGNORE INTO edges(id, source_id, target_id, edge_type, created_at, actor)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![edge_id, source_id, target_id, edge_type, event.ts, event.actor],
            )
            ?;
        }
        "edge.remove" => {
            let edge_id = event
                .payload
                .get("edge_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            conn.execute("DELETE FROM edges WHERE id = ?1", params![edge_id])?;
        }
        "source.add" => {
            let p = &event.payload;
            let src_id = p.get("source_id").and_then(|v| v.as_str()).unwrap_or("");
            let node_id = event.node_id.as_deref().unwrap_or("");
            let source = p.get("source").and_then(|v| v.as_str()).unwrap_or("");

            conn.execute(
                "INSERT OR IGNORE INTO sources(id, node_id, source, created_at) VALUES(?1, ?2, ?3, ?4)",
                params![src_id, node_id, source, event.ts],
            )
            ?;

            // Update node timestamp to match write-time behavior
            conn.execute(
                "UPDATE nodes SET updated_at = ?1 WHERE id = ?2",
                params![event.ts, node_id],
            )?;
        }
        _ => {
            // Unknown event type — skip silently during rebuild
        }
    }

    Ok(())
}

// --- Validation ---

/// Run federation-specific validation gates.
/// Returns a list of (gate_name, passed, message) tuples.
/// Compute a canonical state hash for a federation DB.
/// Stable ordering: nodes by id, sources by (node_id, source), edges by (source_id, edge_type, target_id).
/// Only event-derived timestamps are included — no rebuild-time artifacts.
fn canonical_state_hash(conn: &Connection) -> Result<String, error::DecapodError> {
    let mut hasher = Sha256::new();

    // Nodes sorted by id
    {
        let mut stmt = conn.prepare(
            "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                    created_at, updated_at, effective_from, effective_to, dir_path, actor
             FROM nodes ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            for i in 0..15 {
                let val: String = row.get::<_, Option<String>>(i)?.unwrap_or_default();
                hasher.update(val.as_bytes());
                hasher.update(b"|");
            }
            hasher.update(b"\n");
        }
    }

    // Sources sorted by (node_id, source)
    {
        let mut stmt =
            conn.prepare("SELECT node_id, source FROM sources ORDER BY node_id, source")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let node_id: String = row.get(0)?;
            let source: String = row.get(1)?;
            hasher.update(node_id.as_bytes());
            hasher.update(b"|");
            hasher.update(source.as_bytes());
            hasher.update(b"\n");
        }
    }

    // Edges sorted by (source_id, edge_type, target_id)
    {
        let mut stmt = conn.prepare(
            "SELECT source_id, edge_type, target_id FROM edges ORDER BY source_id, edge_type, target_id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let source_id: String = row.get(0)?;
            let edge_type: String = row.get(1)?;
            let target_id: String = row.get(2)?;
            hasher.update(source_id.as_bytes());
            hasher.update(b"|");
            hasher.update(edge_type.as_bytes());
            hasher.update(b"|");
            hasher.update(target_id.as_bytes());
            hasher.update(b"\n");
        }
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Count nodes, sources, and edges in a federation DB for diff hints.
fn db_counts(conn: &Connection) -> Result<(i64, i64, i64), error::DecapodError> {
    let nodes: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
    let sources: i64 = conn.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))?;
    let edges: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
    Ok((nodes, sources, edges))
}

pub fn validate_federation(
    store_root: &Path,
) -> Result<Vec<(String, bool, String)>, error::DecapodError> {
    let mut results = Vec::new();
    let db_path = federation_db_path(store_root);

    if !db_path.exists() {
        // No federation DB — all gates trivially pass (nothing to validate)
        results.push((
            "federation.store_purity".to_string(),
            true,
            "No federation.db found (clean state)".to_string(),
        ));
        return Ok(results);
    }

    let conn = crate::core::db::db_connect(&db_path.to_string_lossy())?;

    // Gate 1: Store purity — federation.db exists under store root (already true if we got here)
    results.push((
        "federation.store_purity".to_string(),
        true,
        "federation.db located under store root".to_string(),
    ));

    // Gate 2: Provenance — all critical nodes have ≥1 valid source
    {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.node_type, n.priority
                 FROM nodes n
                 WHERE n.status = 'active'
                   AND (n.node_type IN ('decision', 'commitment') OR n.priority = 'critical')
                   AND NOT EXISTS (SELECT 1 FROM sources s WHERE s.node_id = n.id)",
        )?;

        let violations: Vec<String> = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                Ok(format!("{} ({})", id, title))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if violations.is_empty() {
            results.push((
                "federation.provenance".to_string(),
                true,
                "All critical nodes have provenance sources".to_string(),
            ));
        } else {
            results.push((
                "federation.provenance".to_string(),
                false,
                format!(
                    "Critical nodes missing provenance: {}. Fix: decapod data federation sources add --id <node> --source <scheme:ref>. See plugins/FEDERATION.md §4.2",
                    violations.join(", ")
                ),
            ));
        }
    }

    // Gate 3: Write safety — no node.edit events for critical types
    {
        let mut stmt = conn.prepare(
            "SELECT fe.node_id
                 FROM federation_events fe
                 JOIN nodes n ON fe.node_id = n.id
                 WHERE fe.event_type = 'node.edit'
                   AND (n.node_type IN ('decision', 'commitment') OR n.priority = 'critical')",
        )?;

        let violations: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if violations.is_empty() {
            results.push((
                "federation.write_safety".to_string(),
                true,
                "No edit events found for critical nodes".to_string(),
            ));
        } else {
            results.push((
                "federation.write_safety".to_string(),
                false,
                format!(
                    "Critical nodes with edit events (append-only policy violation): {}. Critical types must use 'supersede', not 'edit'. See plugins/FEDERATION.md §6",
                    violations.join(", ")
                ),
            ));
        }
    }

    // Gate 4: Lifecycle DAG — no cycles in supersedes edges
    {
        let mut stmt =
            conn.prepare("SELECT source_id, target_id FROM edges WHERE edge_type = 'supersedes'")?;

        let edges: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let has_cycle = detect_cycle_in_edges(&edges);

        if !has_cycle {
            results.push((
                "federation.lifecycle_dag".to_string(),
                true,
                "Supersedes edges form a DAG (no cycles)".to_string(),
            ));
        } else {
            results.push((
                "federation.lifecycle_dag".to_string(),
                false,
                "Cycle detected in supersedes edges. Run: decapod data federation graph --id <node> --depth 5 to trace. See plugins/FEDERATION.md §7".to_string(),
            ));
        }
    }

    // Gate 5: Rebuild determinism — rebuild to temp DB, compare canonical hashes
    {
        let events_path = federation_events_path(store_root);
        if events_path.exists() {
            // Hash current DB
            let current_hash = canonical_state_hash(&conn)?;
            let (cur_nodes, cur_sources, cur_edges) = db_counts(&conn)?;

            // Rebuild to a unique temp location to avoid collisions across parallel validates.
            let tmp_db = std::env::temp_dir().join(format!(
                "decapod_federation_validate_{}.db",
                ulid::Ulid::new()
            ));
            if tmp_db.exists() {
                let _ = fs::remove_file(&tmp_db);
            }

            let tmp_conn = match crate::core::db::db_connect(&tmp_db.to_string_lossy()) {
                Ok(conn) => conn,
                Err(e) => {
                    results.push((
                        "federation.rebuild_determinism".to_string(),
                        false,
                        format!(
                            "federation.validate rebuild open failed for {}: {}",
                            tmp_db.display(),
                            e
                        ),
                    ));
                    return Ok(results);
                }
            };
            tmp_conn
                .execute("PRAGMA temp_store=MEMORY;", [])
                .map_err(error::DecapodError::RusqliteError)?;
            if let Err(e) = tmp_conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_META) {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "federation.validate rebuild schema failed (meta) for {}: {}",
                        tmp_db.display(),
                        e
                    ),
                ));
                drop(tmp_conn);
                let _ = fs::remove_file(&tmp_db);
                return Ok(results);
            }
            if let Err(e) = tmp_conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_NODES) {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "federation.validate rebuild schema failed (nodes) for {}: {}",
                        tmp_db.display(),
                        e
                    ),
                ));
                drop(tmp_conn);
                let _ = fs::remove_file(&tmp_db);
                return Ok(results);
            }
            if let Err(e) = tmp_conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_SOURCES) {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "federation.validate rebuild schema failed (sources) for {}: {}",
                        tmp_db.display(),
                        e
                    ),
                ));
                drop(tmp_conn);
                let _ = fs::remove_file(&tmp_db);
                return Ok(results);
            }
            if let Err(e) = tmp_conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_EDGES) {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "federation.validate rebuild schema failed (edges) for {}: {}",
                        tmp_db.display(),
                        e
                    ),
                ));
                drop(tmp_conn);
                let _ = fs::remove_file(&tmp_db);
                return Ok(results);
            }
            if let Err(e) = tmp_conn.execute_batch(schemas::FEDERATION_DB_SCHEMA_EVENTS) {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "federation.validate rebuild schema failed (events) for {}: {}",
                        tmp_db.display(),
                        e
                    ),
                ));
                drop(tmp_conn);
                let _ = fs::remove_file(&tmp_db);
                return Ok(results);
            }

            let file = fs::File::open(&events_path).map_err(error::DecapodError::IoError)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line.map_err(error::DecapodError::IoError)?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<FederationEvent>(line) {
                    let _ = replay_event(&tmp_conn, &event);
                }
            }

            let rebuilt_hash = canonical_state_hash(&tmp_conn)?;
            let (reb_nodes, reb_sources, reb_edges) = db_counts(&tmp_conn)?;

            drop(tmp_conn);
            let _ = fs::remove_file(&tmp_db);

            if current_hash == rebuilt_hash {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    true,
                    format!(
                        "DB matches event replay (hash: {}…, {} nodes, {} sources, {} edges)",
                        &current_hash[..12],
                        cur_nodes,
                        cur_sources,
                        cur_edges
                    ),
                ));
            } else {
                results.push((
                    "federation.rebuild_determinism".to_string(),
                    false,
                    format!(
                        "DB diverged from event replay. Current: {} nodes/{} sources/{} edges. Rebuilt: {} nodes/{} sources/{} edges. Run: decapod data federation rebuild",
                        cur_nodes, cur_sources, cur_edges, reb_nodes, reb_sources, reb_edges
                    ),
                ));
            }
        } else {
            results.push((
                "federation.rebuild_determinism".to_string(),
                true,
                "No events file found (clean state)".to_string(),
            ));
        }
    }

    // Gate 6: Derived index freshness — federation/_index.md matches deterministic render
    {
        let (node_count, _source_count, edge_count) = db_counts(&conn)?;
        if node_count == 0 && edge_count == 0 {
            results.push((
                "federation.derived_index_fresh".to_string(),
                true,
                "No nodes/edges found (clean state)".to_string(),
            ));
        } else {
            let path = federation_index_path(store_root);
            let expected = build_index_markdown(&conn)?;
            match fs::read_to_string(&path) {
                Ok(actual) => {
                    if actual == expected {
                        results.push((
                            "federation.derived_index_fresh".to_string(),
                            true,
                            "Derived index is fresh".to_string(),
                        ));
                    } else {
                        results.push((
                        "federation.derived_index_fresh".to_string(),
                        false,
                        "Derived index drift detected. Run: decapod data federation index-build"
                            .to_string(),
                    ));
                    }
                }
                Err(_) => {
                    results.push((
                        "federation.derived_index_fresh".to_string(),
                        false,
                        "Derived index missing. Run: decapod data federation index-build"
                            .to_string(),
                    ));
                }
            }
        }
    }

    // Gate 7: Derived graph freshness — federation/_graph.json matches deterministic render
    {
        let (node_count, _source_count, edge_count) = db_counts(&conn)?;
        if node_count == 0 && edge_count == 0 {
            results.push((
                "federation.derived_graph_fresh".to_string(),
                true,
                "No nodes/edges found (clean state)".to_string(),
            ));
        } else {
            let path = federation_graph_path(store_root);
            let expected = serde_json::to_string_pretty(&build_graph_json(&conn)?).unwrap();
            match fs::read_to_string(&path) {
                Ok(actual) => {
                    if actual == expected {
                        results.push((
                            "federation.derived_graph_fresh".to_string(),
                            true,
                            "Derived graph is fresh".to_string(),
                        ));
                    } else {
                        results.push((
                        "federation.derived_graph_fresh".to_string(),
                        false,
                        "Derived graph drift detected. Run: decapod data federation graph-export"
                            .to_string(),
                    ));
                    }
                }
                Err(_) => {
                    results.push((
                        "federation.derived_graph_fresh".to_string(),
                        false,
                        "Derived graph missing. Run: decapod data federation graph-export"
                            .to_string(),
                    ));
                }
            }
        }
    }

    Ok(results)
}

fn detect_cycle_in_edges(edges: &[(String, String)]) -> bool {
    use std::collections::{HashMap, HashSet};

    // Build adjacency list
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        adj.entry(from.as_str()).or_default().push(to.as_str());
    }

    // DFS cycle detection
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    fn dfs<'a>(
        node: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        in_stack: &mut HashSet<&'a str>,
    ) -> bool {
        visited.insert(node);
        in_stack.insert(node);

        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                if !visited.contains(next) {
                    if dfs(next, adj, visited, in_stack) {
                        return true;
                    }
                } else if in_stack.contains(next) {
                    return true;
                }
            }
        }

        in_stack.remove(node);
        false
    }

    let all_nodes: HashSet<&str> = edges
        .iter()
        .flat_map(|(a, b)| vec![a.as_str(), b.as_str()])
        .collect();

    for node in &all_nodes {
        if !visited.contains(node) && dfs(node, &adj, &mut visited, &mut in_stack) {
            return true;
        }
    }

    false
}

// --- CLI Runner ---

pub fn run_federation_cli(store: &Store, cli: FederationCli) -> Result<(), error::DecapodError> {
    initialize_federation_db(&store.root)?;

    match cli.command {
        FederationCommand::Add {
            title,
            node_type,
            priority,
            confidence,
            body,
            sources,
            tags,
            scope,
            effective_from,
            actor,
        } => {
            let node = add_node(
                store,
                &title,
                &node_type,
                &priority,
                &confidence,
                &body,
                &sources,
                &tags,
                &scope,
                effective_from.as_deref(),
                &actor,
            )?;

            match cli.format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&node).unwrap());
                }
                OutputFormat::Text => {
                    println!("Node created: {} ({})", node.id, node.title);
                }
            }
        }

        FederationCommand::Get { id } => {
            let broker = DbBroker::new(&store.root);
            let db_path = federation_db_path(&store.root);

            let node = broker.with_conn(&db_path, "decapod", None, "federation.get", |conn| {
                read_node_full(conn, &id)
            })?;

            match cli.format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&node).unwrap());
                }
                OutputFormat::Text => {
                    println!("ID:         {}", node.id);
                    println!("Title:      {}", node.title);
                    println!("Type:       {}", node.node_type);
                    println!("Status:     {}", node.status);
                    println!("Priority:   {}", node.priority);
                    println!("Confidence: {}", node.confidence);
                    println!("Scope:      {}", node.scope);
                    println!("Tags:       {}", node.tags);
                    println!("Created:    {}", node.created_at);
                    println!("Updated:    {}", node.updated_at);
                    if let Some(ref ef) = node.effective_from {
                        println!("Effective:  {}", ef);
                    }
                    if let Some(ref et) = node.effective_to {
                        println!("Expired:    {}", et);
                    }
                    println!("Actor:      {}", node.actor);
                    if let Some(ref sources) = node.sources {
                        if !sources.is_empty() {
                            println!("Sources:");
                            for s in sources {
                                println!("  - {}", s);
                            }
                        }
                    }
                    if let Some(ref edges) = node.edges {
                        if !edges.is_empty() {
                            println!("Edges:");
                            for e in edges {
                                println!(
                                    "  {} --[{}]--> {}",
                                    e.source_id, e.edge_type, e.target_id
                                );
                            }
                        }
                    }
                    if !node.body.is_empty() {
                        println!("\n{}", node.body);
                    }
                }
            }
        }

        FederationCommand::List {
            node_type,
            status,
            priority,
            scope,
        } => {
            let broker = DbBroker::new(&store.root);
            let db_path = federation_db_path(&store.root);

            let nodes = broker.with_conn(&db_path, "decapod", None, "federation.list", |conn| {
                let mut conditions = vec!["1=1".to_string()];
                let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];
                let mut idx = 1u32;

                if let Some(ref nt) = node_type {
                    conditions.push(format!("node_type = ?{}", idx));
                    param_values.push(Box::new(nt.clone()));
                    idx += 1;
                }
                if let Some(ref s) = status {
                    conditions.push(format!("status = ?{}", idx));
                    param_values.push(Box::new(s.clone()));
                    idx += 1;
                }
                if let Some(ref p) = priority {
                    conditions.push(format!("priority = ?{}", idx));
                    param_values.push(Box::new(p.clone()));
                    idx += 1;
                }
                if let Some(ref sc) = scope {
                    conditions.push(format!("scope = ?{}", idx));
                    param_values.push(Box::new(sc.clone()));
                    idx += 1;
                }
                let _ = idx;

                let sql = format!(
                    "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                                created_at, updated_at, effective_from, effective_to, actor
                         FROM nodes WHERE {} ORDER BY updated_at DESC",
                    conditions.join(" AND ")
                );

                let mut stmt = conn.prepare(&sql)?;
                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    param_values.iter().map(|b| b.as_ref()).collect();

                let rows = stmt.query_map(params_refs.as_slice(), |row| {
                    Ok(FederationNode {
                        id: row.get(0)?,
                        node_type: row.get(1)?,
                        status: row.get(2)?,
                        priority: row.get(3)?,
                        confidence: row.get(4)?,
                        title: row.get(5)?,
                        body: row.get(6)?,
                        scope: row.get(7)?,
                        tags: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        effective_from: row.get(11)?,
                        effective_to: row.get(12)?,
                        actor: row.get(13)?,
                        sources: None,
                        edges: None,
                    })
                })?;

                let mut nodes = Vec::new();
                for r in rows {
                    nodes.push(r?);
                }
                Ok(nodes)
            })?;

            match cli.format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&nodes).unwrap());
                }
                OutputFormat::Text => {
                    if nodes.is_empty() {
                        println!("No nodes found.");
                    } else {
                        for n in &nodes {
                            println!(
                                "[{}] {} | {} | {} | {}",
                                n.status, n.id, n.node_type, n.priority, n.title
                            );
                        }
                        println!("\n{} node(s)", nodes.len());
                    }
                }
            }
        }

        FederationCommand::Search { query, scope } => {
            let broker = DbBroker::new(&store.root);
            let db_path = federation_db_path(&store.root);

            let nodes =
                broker.with_conn(&db_path, "decapod", None, "federation.search", |conn| {
                    let q = format!("%{}%", query);
                    let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
                        if let Some(ref sc) = scope {
                            (
                                "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                                        created_at, updated_at, effective_from, effective_to, actor
                                 FROM nodes WHERE (title LIKE ?1 OR body LIKE ?1) AND scope = ?2
                                 ORDER BY updated_at DESC".to_string(),
                                vec![Box::new(q), Box::new(sc.clone())],
                            )
                        } else {
                            (
                                "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                                        created_at, updated_at, effective_from, effective_to, actor
                                 FROM nodes WHERE title LIKE ?1 OR body LIKE ?1
                                 ORDER BY updated_at DESC".to_string(),
                                vec![Box::new(q)],
                            )
                        };

                    let mut stmt = conn.prepare(&sql)?;
                    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                        param_values.iter().map(|b| b.as_ref()).collect();

                    let rows = stmt
                        .query_map(params_refs.as_slice(), |row| {
                            Ok(FederationNode {
                                id: row.get(0)?,
                                node_type: row.get(1)?,
                                status: row.get(2)?,
                                priority: row.get(3)?,
                                confidence: row.get(4)?,
                                title: row.get(5)?,
                                body: row.get(6)?,
                                scope: row.get(7)?,
                                tags: row.get(8)?,
                                created_at: row.get(9)?,
                                updated_at: row.get(10)?,
                                effective_from: row.get(11)?,
                                effective_to: row.get(12)?,
                                actor: row.get(13)?,
                                sources: None,
                                edges: None,
                            })
                        })
                        ?;

                    let mut nodes = Vec::new();
                    for r in rows {
                        nodes.push(r?);
                    }
                    Ok(nodes)
                })?;

            match cli.format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&nodes).unwrap());
                }
                OutputFormat::Text => {
                    if nodes.is_empty() {
                        println!("No results for '{}'.", query);
                    } else {
                        for n in &nodes {
                            println!(
                                "[{}] {} | {} | {} | {}",
                                n.status, n.id, n.node_type, n.priority, n.title
                            );
                        }
                        println!("\n{} result(s)", nodes.len());
                    }
                }
            }
        }

        FederationCommand::Edit {
            id,
            title,
            body,
            tags,
            priority,
        } => {
            edit_node(
                store,
                &id,
                title.as_deref(),
                body.as_deref(),
                tags.as_deref(),
                priority.as_deref(),
            )?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "id": id, "op": "edit"})
                    );
                }
                OutputFormat::Text => {
                    println!("Node '{}' updated.", id);
                }
            }
        }

        FederationCommand::Supersede { id, by, reason } => {
            supersede_node(store, &id, &by, &reason)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "ok",
                            "op": "supersede",
                            "old_id": id,
                            "new_id": by,
                        })
                    );
                }
                OutputFormat::Text => {
                    println!("Node '{}' superseded by '{}'.", id, by);
                }
            }
        }

        FederationCommand::Deprecate { id, reason } => {
            transition_node_status(store, &id, "deprecated", "node.deprecate", &reason)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "id": id, "op": "deprecate"})
                    );
                }
                OutputFormat::Text => {
                    println!("Node '{}' deprecated.", id);
                }
            }
        }

        FederationCommand::Dispute { id, reason } => {
            transition_node_status(store, &id, "disputed", "node.dispute", &reason)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "id": id, "op": "dispute"})
                    );
                }
                OutputFormat::Text => {
                    println!("Node '{}' disputed.", id);
                }
            }
        }

        FederationCommand::Link {
            source,
            target,
            edge_type,
        } => {
            let edge_id = add_edge(store, &source, &target, &edge_type)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "ok",
                            "edge_id": edge_id,
                            "source": source,
                            "target": target,
                            "edge_type": edge_type,
                        })
                    );
                }
                OutputFormat::Text => {
                    println!(
                        "Edge created: {} --[{}]--> {} ({})",
                        source, edge_type, target, edge_id
                    );
                }
            }
        }

        FederationCommand::Unlink { id } => {
            remove_edge(store, &id)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "edge_id": id, "op": "unlink"})
                    );
                }
                OutputFormat::Text => {
                    println!("Edge '{}' removed.", id);
                }
            }
        }

        FederationCommand::Graph { id, depth } => {
            let result = graph_neighbors(store, &id, depth)?;

            match cli.format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                }
                OutputFormat::Text => {
                    let nodes = result
                        .get("nodes")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let edges = result
                        .get("edges")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    println!(
                        "Graph from '{}' (depth {}): {} nodes, {} edges",
                        id, depth, nodes, edges
                    );
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                }
            }
        }

        FederationCommand::Rebuild => {
            let count = rebuild_from_events(&store.root)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "events_replayed": count})
                    );
                }
                OutputFormat::Text => {
                    println!(
                        "Federation DB rebuilt from events ({} events replayed).",
                        count
                    );
                }
            }
        }

        FederationCommand::SourcesAdd { id, source } => {
            let src_id = add_source_to_node(store, &id, &source)?;

            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "ok",
                            "op": "sources.add",
                            "node_id": id,
                            "source_id": src_id,
                            "source": source,
                        })
                    );
                }
                OutputFormat::Text => {
                    println!("Source added to node '{}': {} ({})", id, source, src_id);
                }
            }
        }

        FederationCommand::Init => {
            // initialize_federation_db is already called at the top of run_federation_cli
            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"status": "ok", "op": "init", "store": store.root.to_string_lossy()})
                    );
                }
                OutputFormat::Text => {
                    println!("Federation initialized at {}", store.root.to_string_lossy());
                }
            }
        }

        FederationCommand::VaultExport => {
            let count = export_vault_notes(store)?;
            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status":"ok",
                            "op":"vault.export",
                            "nodes_exported":count,
                            "path":federation_vault_dir(&store.root)
                        })
                    );
                }
                OutputFormat::Text => {
                    println!(
                        "Vault exported: {} note(s) under {}",
                        count,
                        federation_vault_dir(&store.root).to_string_lossy()
                    );
                }
            }
        }

        FederationCommand::IndexBuild => {
            let lines = build_index_file(store)?;
            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status":"ok",
                            "op":"index.build",
                            "lines":lines,
                            "path":federation_index_path(&store.root)
                        })
                    );
                }
                OutputFormat::Text => {
                    println!(
                        "Index built ({} lines): {}",
                        lines,
                        federation_index_path(&store.root).to_string_lossy()
                    );
                }
            }
        }

        FederationCommand::GraphExport => {
            let (nodes, edges) = export_graph_file(store)?;
            match cli.format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status":"ok",
                            "op":"graph.export",
                            "nodes":nodes,
                            "edges":edges,
                            "path":federation_graph_path(&store.root)
                        })
                    );
                }
                OutputFormat::Text => {
                    println!(
                        "Graph exported: {} nodes, {} edges -> {}",
                        nodes,
                        edges,
                        federation_graph_path(&store.root).to_string_lossy()
                    );
                }
            }
        }

        FederationCommand::Schema => {
            println!("{}", serde_json::to_string_pretty(&schema()).unwrap());
        }
    }

    Ok(())
}

// --- Schema Export ---

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "federation",
        "version": "0.1.0",
        "description": "Governed agent memory — typed knowledge graph with provenance and lifecycle",
        "node_types": VALID_NODE_TYPES,
        "critical_types": CRITICAL_NODE_TYPES,
        "statuses": VALID_STATUSES,
        "priorities": VALID_PRIORITIES,
        "confidences": VALID_CONFIDENCES,
        "edge_types": VALID_EDGE_TYPES,
        "commands": [
            {"name": "add", "description": "Create a new memory node"},
            {"name": "get", "description": "Get a node by ID with sources and edges"},
            {"name": "list", "description": "List nodes with filters"},
            {"name": "search", "description": "Search nodes by title and body"},
            {"name": "edit", "description": "Edit non-critical node fields"},
            {"name": "supersede", "description": "Supersede a node with a replacement"},
            {"name": "deprecate", "description": "Mark a node as deprecated"},
            {"name": "dispute", "description": "Mark a node as disputed"},
            {"name": "link", "description": "Add a typed edge between nodes"},
            {"name": "unlink", "description": "Remove an edge"},
            {"name": "graph", "description": "Show node neighborhood"},
            {"name": "vault-export", "description": "Export vault markdown notes under federation/vault"},
            {"name": "index-build", "description": "Build deterministic federation/_index.md"},
            {"name": "graph-export", "description": "Build deterministic federation/_graph.json"},
            {"name": "rebuild", "description": "Rebuild DB from event log"},
            {"name": "schema", "description": "Print JSON schema"},
        ],
        "storage": ["federation.db", "federation.events.jsonl", "federation/_index.md", "federation/_graph.json", "federation/vault/"],
        "provenance_schemes": ["file:", "url:", "cmd:", "commit:", "event:"],
    })
}
