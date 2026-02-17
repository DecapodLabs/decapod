use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::store::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub claim_id: Option<String>,
    pub merge_key: Option<String>,
    pub status: String,
    pub ttl_policy: String,
    pub expires_ts: Option<String>,
    pub supersedes_id: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub recency_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KnowledgeConflictPolicy {
    Merge,
    Supersede,
    Reject,
}

#[derive(Debug, Clone)]
pub struct AddKnowledgeParams<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub provenance: &'a str,
    pub claim_id: Option<&'a str>,
    pub merge_key: Option<&'a str>,
    pub conflict_policy: KnowledgeConflictPolicy,
    pub status: &'a str,
    pub ttl_policy: &'a str,
    pub expires_ts: Option<&'a str>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddKnowledgeResult {
    pub id: String,
    pub action: String,
    pub superseded_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SearchOptions<'a> {
    pub as_of: Option<&'a str>,
    pub window_days: Option<u32>,
    pub rank: &'a str,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RetrievalFeedbackResult {
    pub event_id: String,
    pub file: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DecayResult {
    pub as_of: String,
    pub policy: String,
    pub dry_run: bool,
    pub stale_ids: Vec<String>,
    pub event_id: String,
}

pub fn parse_conflict_policy(value: &str) -> Result<KnowledgeConflictPolicy, error::DecapodError> {
    match value {
        "merge" => Ok(KnowledgeConflictPolicy::Merge),
        "supersede" => Ok(KnowledgeConflictPolicy::Supersede),
        "reject" => Ok(KnowledgeConflictPolicy::Reject),
        other => Err(error::DecapodError::ValidationError(format!(
            "Invalid conflict policy '{}'. Expected merge|supersede|reject",
            other
        ))),
    }
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join("knowledge.db")
}

pub fn add_knowledge(
    store: &Store,
    args: AddKnowledgeParams<'_>,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    use regex::Regex;
    let prov_re = Regex::new(
        r"^(file:[^#]+(#L\d+(-L\d+)?)?|url:[^ ]+|cmd:[^ ]+|commit:[a-f0-9]+|event:[A-Z0-9_]+)$",
    )
    .unwrap();

    if !prov_re.is_match(args.provenance) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid provenance format: '{}'. Must match scheme (file:|url:|cmd:|commit:|event:)",
            args.provenance
        )));
    }

    if !matches!(
        args.status,
        "active" | "superseded" | "deprecated" | "stale"
    ) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid knowledge status '{}'. Expected active|superseded|deprecated|stale",
            args.status
        )));
    }

    if !matches!(args.ttl_policy, "ephemeral" | "decay" | "persistent") {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid ttl_policy '{}'. Expected ephemeral|decay|persistent",
            args.ttl_policy
        )));
    }

    if let Some(expires_ts) = args.expires_ts {
        parse_epoch_z(expires_ts)?;
    }

    let broker = DbBroker::new(&store.root);
    let db_path = knowledge_db_path(&store.root);
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "knowledge.add", |conn| {
        let mut action = "inserted".to_string();
        let mut effective_id = args.id.to_string();
        let mut superseded_ids = Vec::new();

        if let Some(merge_key) = args.merge_key {
            let existing = conn.query_row(
                "SELECT id FROM knowledge WHERE merge_key = ?1 AND status = 'active' AND scope = ?2",
                params![merge_key, "root"],
                |row| row.get::<_, String>(0),
            );

            if let Ok(existing_id) = existing {
                match args.conflict_policy {
                    KnowledgeConflictPolicy::Merge => {
                        conn.execute(
                            "UPDATE knowledge
                             SET title = ?2, content = ?3, provenance = ?4, claim_id = ?5,
                                 ttl_policy = ?6, expires_ts = ?7, updated_at = ?8
                             WHERE id = ?1",
                            params![
                                existing_id,
                                args.title,
                                args.content,
                                args.provenance,
                                args.claim_id,
                                args.ttl_policy,
                                args.expires_ts,
                                now
                            ],
                        )?;
                        action = "merged".to_string();
                        effective_id = existing_id;
                    }
                    KnowledgeConflictPolicy::Supersede => {
                        conn.execute(
                            "UPDATE knowledge SET status = 'superseded', updated_at = ?2 WHERE id = ?1",
                            params![existing_id, now],
                        )?;
                        superseded_ids.push(existing_id.clone());
                        conn.execute(
                            "INSERT INTO knowledge(id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope, status, merge_key, supersedes_id, ttl_policy, expires_ts)
                             VALUES(?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                            params![
                                args.id,
                                args.title,
                                args.content,
                                args.provenance,
                                args.claim_id,
                                now,
                                now,
                                store.root.to_string_lossy(),
                                "root",
                                args.status,
                                args.merge_key,
                                Some(existing_id),
                                args.ttl_policy,
                                args.expires_ts
                            ],
                        )?;
                        action = "superseded".to_string();
                        effective_id = args.id.to_string();
                    }
                    KnowledgeConflictPolicy::Reject => {
                        return Err(error::DecapodError::ValidationError(
                            "knowledge merge_key conflict: active entry already exists and on_conflict=reject"
                                .to_string(),
                        ));
                    }
                }
            } else {
                conn.execute(
                    "INSERT INTO knowledge(id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope, status, merge_key, supersedes_id, ttl_policy, expires_ts)
                     VALUES(?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        args.id,
                        args.title,
                        args.content,
                        args.provenance,
                        args.claim_id,
                        now,
                        now,
                        store.root.to_string_lossy(),
                        "root",
                        args.status,
                        args.merge_key,
                        Option::<String>::None,
                        args.ttl_policy,
                        args.expires_ts
                    ],
                )?;
            }
        } else {
            conn.execute(
                "INSERT INTO knowledge(id, title, content, provenance, claim_id, tags, created_at, updated_at, dir_path, scope, status, merge_key, supersedes_id, ttl_policy, expires_ts)
                 VALUES(?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?9, ?10, '', ?11, ?12, ?13)",
                params![
                    args.id,
                    args.title,
                    args.content,
                    args.provenance,
                    args.claim_id,
                    now,
                    now,
                    store.root.to_string_lossy(),
                    "root",
                    args.status,
                    Option::<String>::None,
                    args.ttl_policy,
                    args.expires_ts
                ],
            )?;
        }

        Ok(AddKnowledgeResult {
            id: effective_id,
            action,
            superseded_ids,
        })
    })
}

pub fn search_knowledge(
    store: &Store,
    query: &str,
    options: SearchOptions<'_>,
) -> Result<Vec<KnowledgeEntry>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = knowledge_db_path(&store.root);

    let mut rows = broker.with_conn(&db_path, "decapod", None, "knowledge.search", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, provenance, claim_id, created_at, updated_at,
                    status, merge_key, ttl_policy, expires_ts, supersedes_id
             FROM knowledge
             WHERE (title LIKE ?1 OR content LIKE ?1 OR provenance LIKE ?1)
               AND status = 'active'",
        )?;
        let q = format!("%{}%", query);
        let mapped = stmt.query_map(params![q], |row| {
            Ok(KnowledgeEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                provenance: row.get(3)?,
                claim_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                status: row.get(7)?,
                merge_key: row.get(8)?,
                ttl_policy: row.get(9)?,
                expires_ts: row.get(10)?,
                supersedes_id: row.get(11)?,
                recency_score: None,
            })
        })?;

        let mut items = Vec::new();
        for item in mapped {
            items.push(item?);
        }
        Ok(items)
    })?;

    let as_of_ts = options
        .as_of
        .map(parse_epoch_z)
        .transpose()?
        .unwrap_or_else(now_epoch_secs);

    rows.retain(|entry| {
        parse_epoch_z(&entry.created_at)
            .map(|created| created <= as_of_ts)
            .unwrap_or(false)
    });

    if let Some(window_days) = options.window_days {
        let min_ts = as_of_ts.saturating_sub((window_days as i64) * 86_400);
        rows.retain(|entry| {
            parse_epoch_z(&entry.created_at)
                .map(|created| created >= min_ts)
                .unwrap_or(false)
        });
    }

    match options.rank {
        "recency_decay" => {
            for entry in &mut rows {
                let created = parse_epoch_z(&entry.created_at).unwrap_or(as_of_ts);
                let age_days = ((as_of_ts - created).max(0) as f64) / 86_400.0;
                entry.recency_score = Some(1.0 / (1.0 + age_days));
            }
            rows.sort_by(|a, b| {
                b.recency_score
                    .partial_cmp(&a.recency_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        _ => {
            rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        }
    }

    Ok(rows)
}

pub fn log_retrieval_feedback(
    store: &Store,
    actor: &str,
    query: &str,
    returned_ids: &[String],
    used_ids: &[String],
    outcome: &str,
) -> Result<RetrievalFeedbackResult, error::DecapodError> {
    if !matches!(outcome, "helped" | "neutral" | "hurt" | "unknown") {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid outcome '{}'. Expected helped|neutral|hurt|unknown",
            outcome
        )));
    }

    let file = store.root.join("memory.retrieval_events.jsonl");
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(error::DecapodError::IoError)?;

    let event_id = crate::core::time::new_event_id();
    let payload = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "store": match store.kind {
            crate::core::store::StoreKind::User => "user",
            crate::core::store::StoreKind::Repo => "repo",
        },
        "actor": actor,
        "query": query,
        "returned_ids": returned_ids,
        "used_ids": used_ids,
        "outcome": outcome
    });

    writeln!(f, "{}", payload).map_err(error::DecapodError::IoError)?;

    Ok(RetrievalFeedbackResult {
        event_id,
        file: file.display().to_string(),
    })
}

pub fn decay_knowledge(
    store: &Store,
    policy: &str,
    as_of: Option<&str>,
    dry_run: bool,
) -> Result<DecayResult, error::DecapodError> {
    let as_of_owned = as_of.map(|s| s.to_string()).unwrap_or_else(now_iso);
    let as_of = as_of_owned.as_str();
    let as_of_secs = parse_epoch_z(as_of)?;
    let db_path = knowledge_db_path(&store.root);
    let broker = DbBroker::new(&store.root);

    let stale_ids = broker.with_conn(&db_path, "decapod", None, "knowledge.decay", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, created_at, ttl_policy, expires_ts FROM knowledge WHERE status = 'active'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        let mut stale_ids = Vec::new();
        for row in rows {
            let (id, created_at, ttl_policy, expires_ts) = row?;
            if ttl_policy == "persistent" {
                continue;
            }

            let ttl_secs = match ttl_policy.as_str() {
                "ephemeral" => 7 * 86_400,
                "decay" => 30 * 86_400,
                _ => continue,
            };

            let created_secs = parse_epoch_z(&created_at).unwrap_or(as_of_secs);
            let expiry = expires_ts
                .as_deref()
                .and_then(|v| parse_epoch_z(v).ok())
                .unwrap_or(created_secs.saturating_add(ttl_secs));

            if as_of_secs >= expiry {
                stale_ids.push(id.clone());
                if !dry_run {
                    conn.execute(
                        "UPDATE knowledge SET status = 'stale', updated_at = ?2 WHERE id = ?1",
                        params![id, as_of],
                    )?;
                }
            }
        }

        Ok(stale_ids)
    })?;

    let event_id = crate::core::time::new_event_id();
    let decay_event_file = store.root.join("memory.decay_events.jsonl");
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&decay_event_file)
        .map_err(error::DecapodError::IoError)?;
    let event = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "policy": policy,
        "as_of": as_of,
        "dry_run": dry_run,
        "stale_ids": stale_ids
    });
    writeln!(f, "{}", event).map_err(error::DecapodError::IoError)?;

    Ok(DecayResult {
        as_of: as_of.to_string(),
        policy: policy.to_string(),
        dry_run,
        stale_ids: event["stale_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        event_id,
    })
}

fn parse_epoch_z(value: &str) -> Result<i64, error::DecapodError> {
    let s = value.trim_end_matches('Z');
    s.parse::<i64>().map_err(|_| {
        error::DecapodError::ValidationError(format!(
            "Expected epoch timestamp with optional Z suffix, got '{}'",
            value
        ))
    })
}

fn now_epoch_secs() -> i64 {
    now_iso()
        .trim_end_matches('Z')
        .parse::<i64>()
        .unwrap_or_default()
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "knowledge",
        "version": "0.2.0",
        "description": "Repository context, merge/supersede lifecycle, retrieval ROI events",
        "commands": [
            {
                "name": "add",
                "description": "Add a knowledge entry with merge/supersede semantics",
                "parameters": [
                    {"name": "id", "required": true, "description": "Unique knowledge entry ID (ULID or UUID)"},
                    {"name": "title", "required": true, "description": "Short, specific title for the entry"},
                    {"name": "text", "required": true, "description": "Main content/markdown body of the knowledge entry"},
                    {"name": "provenance", "required": true, "description": "Source reference (file:, url:, cmd:, commit:, or event: format required)"},
                    {"name": "claim_id", "required": false, "description": "Optional claim ID this knowledge relates to"},
                    {"name": "merge_key", "required": false, "description": "Deterministic merge identity for active-entry de-duplication"},
                    {"name": "on_conflict", "required": false, "description": "Conflict policy: merge|supersede|reject"},
                    {"name": "ttl_policy", "required": false, "description": "TTL policy: ephemeral|decay|persistent"}
                ]
            },
            {
                "name": "search",
                "description": "Search active knowledge entries with temporal filters",
                "parameters": [
                    {"name": "query", "required": true, "description": "Search query for title, content, or provenance"},
                    {"name": "as_of", "required": false, "description": "Exclude entries newer than this epoch timestamp"},
                    {"name": "window_days", "required": false, "description": "Limit results to this many days before as_of"},
                    {"name": "rank", "required": false, "description": "Ranking mode: recency_desc|recency_decay"}
                ]
            },
            {
                "name": "retrieval-log",
                "description": "Log retrieval feedback outcome for ROI tracking",
                "parameters": [
                    {"name": "query", "required": true, "description": "Query text used for retrieval"},
                    {"name": "returned_ids", "required": true, "description": "Comma-separated returned IDs"},
                    {"name": "used_ids", "required": false, "description": "Comma-separated IDs actually used"},
                    {"name": "outcome", "required": true, "description": "helped|neutral|hurt|unknown"}
                ]
            },
            {
                "name": "decay",
                "description": "Apply deterministic TTL decay/prune policy to active entries",
                "parameters": [
                    {"name": "policy", "required": false, "description": "Named decay policy (default)"},
                    {"name": "as_of", "required": false, "description": "Epoch timestamp used for deterministic decay replay"},
                    {"name": "dry_run", "required": false, "description": "Preview stale IDs without mutating status"}
                ]
            }
        ],
        "storage": ["knowledge.db", "memory.retrieval_events.jsonl", "memory.decay_events.jsonl"]
    })
}
