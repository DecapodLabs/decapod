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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgePromotionEvent {
    pub event_id: String,
    pub ts: String,
    pub source_entry_id: String,
    pub target_class: String,
    pub evidence_refs: Vec<String>,
    pub approved_by: String,
    pub actor: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgePromotionEventInput<'a> {
    pub source_entry_id: &'a str,
    pub evidence_refs: &'a [String],
    pub approved_by: &'a str,
    pub actor: &'a str,
    pub reason: &'a str,
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
        let rows = stmt.query_map(params![q], |row| {
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

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    })?;

    // Apply as_of temporal filtering
    if let Some(as_of) = options.as_of {
        let cutoff_secs = parse_epoch_z(as_of)?;
        rows.retain(|e| {
            let created_secs = e
                .created_at
                .trim_end_matches('Z')
                .parse::<u64>()
                .unwrap_or(0);
            created_secs <= cutoff_secs
        });
    }

    // Apply window_days filter relative to as_of or now
    if let Some(window) = options.window_days {
        let ref_secs = if let Some(as_of) = options.as_of {
            parse_epoch_z(as_of).unwrap_or(0)
        } else {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        };
        let min_secs = ref_secs.saturating_sub(u64::from(window) * 86400);
        rows.retain(|e| {
            let created_secs = e
                .created_at
                .trim_end_matches('Z')
                .parse::<u64>()
                .unwrap_or(0);
            created_secs >= min_secs
        });
    }

    // Apply recency scoring
    if options.rank == "recency_decay" {
        let now_secs = {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as f64
        };
        for entry in &mut rows {
            let created_secs = entry
                .created_at
                .trim_end_matches('Z')
                .parse::<f64>()
                .unwrap_or(0.0);
            let age_days = (now_secs - created_secs) / 86400.0;
            entry.recency_score = Some(1.0 / (1.0 + age_days));
        }
        rows.sort_by(|a, b| {
            b.recency_score
                .unwrap_or(0.0)
                .partial_cmp(&a.recency_score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    Ok(rows)
}

/// Log a retrieval feedback event (append-only).
pub fn log_retrieval_feedback(
    store: &Store,
    query: &str,
    returned_ids: &[String],
    used_ids: &[String],
    outcome: &str,
    source: &str,
) -> Result<RetrievalFeedbackResult, error::DecapodError> {
    if !matches!(outcome, "helped" | "neutral" | "hurt" | "unknown") {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid retrieval outcome '{}'. Expected helped|neutral|hurt|unknown",
            outcome
        )));
    }
    if !matches!(source, "invocation" | "manual_feedback") {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid retrieval source '{}'. Expected invocation|manual_feedback",
            source
        )));
    }

    let event_id = ulid::Ulid::new().to_string();
    let event = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "query": query,
        "returned_ids": returned_ids,
        "used_ids": used_ids,
        "outcome": outcome,
        "source": source,
    });

    let events_path = store.root.join("knowledge.retrieval.events.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_path)
        .map_err(error::DecapodError::IoError)?;
    let line = serde_json::to_string(&event)
        .map_err(|e| error::DecapodError::ValidationError(format!("JSON error: {}", e)))?;
    writeln!(file, "{}", line).map_err(error::DecapodError::IoError)?;

    Ok(RetrievalFeedbackResult {
        event_id,
        file: events_path.to_string_lossy().to_string(),
    })
}

/// Run decay/prune on knowledge entries with ttl_policy=decay or ephemeral.
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
            "SELECT id, expires_ts FROM knowledge
             WHERE status = 'active' AND ttl_policy IN ('decay', 'ephemeral')
               AND expires_ts IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut stale = Vec::new();
        for row in rows {
            let (id, exp_ts) = row?;
            let exp_secs = exp_ts
                .trim_end_matches('Z')
                .parse::<u64>()
                .unwrap_or(u64::MAX);
            if exp_secs <= as_of_secs {
                stale.push(id);
            }
        }

        if !dry_run && !stale.is_empty() {
            for id in &stale {
                conn.execute(
                    "UPDATE knowledge SET status = 'stale', updated_at = ?2 WHERE id = ?1",
                    params![id, as_of],
                )?;
            }
        }

        Ok(stale)
    })?;

    // Log decay event
    let event_id = ulid::Ulid::new().to_string();
    let event = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "policy": policy,
        "as_of": as_of,
        "dry_run": dry_run,
        "stale_ids": stale_ids,
    });

    let events_path = store.root.join("knowledge.decay.events.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_path)
        .map_err(error::DecapodError::IoError)?;
    let line = serde_json::to_string(&event)
        .map_err(|e| error::DecapodError::ValidationError(format!("JSON error: {}", e)))?;
    writeln!(file, "{}", line).map_err(error::DecapodError::IoError)?;

    Ok(DecayResult {
        as_of: as_of.to_string(),
        policy: policy.to_string(),
        dry_run,
        stale_ids,
        event_id,
    })
}

pub fn record_promotion_event(
    store: &Store,
    input: KnowledgePromotionEventInput<'_>,
) -> Result<KnowledgePromotionEvent, error::DecapodError> {
    if input.source_entry_id.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "source_entry_id is required".to_string(),
        ));
    }
    if input.evidence_refs.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "at least one --evidence-ref is required".to_string(),
        ));
    }
    if input.approved_by.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "approved_by is required".to_string(),
        ));
    }
    if input.actor.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "actor is required".to_string(),
        ));
    }
    if input.reason.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "reason is required".to_string(),
        ));
    }

    let mut evidence_refs = input
        .evidence_refs
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    evidence_refs.sort();
    evidence_refs.dedup();
    if evidence_refs.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "evidence_refs cannot be empty".to_string(),
        ));
    }

    let event = KnowledgePromotionEvent {
        event_id: ulid::Ulid::new().to_string(),
        ts: now_iso(),
        source_entry_id: input.source_entry_id.trim().to_string(),
        target_class: "procedural".to_string(),
        evidence_refs,
        approved_by: input.approved_by.trim().to_string(),
        actor: input.actor.trim().to_string(),
        reason: input.reason.trim().to_string(),
    };

    let ledger_path = store.root.join("knowledge.promotions.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .map_err(error::DecapodError::IoError)?;
    let line = serde_json::to_string(&event)
        .map_err(|e| error::DecapodError::ValidationError(format!("JSON error: {}", e)))?;
    writeln!(file, "{}", line).map_err(error::DecapodError::IoError)?;

    Ok(event)
}

fn parse_epoch_z(ts: &str) -> Result<u64, error::DecapodError> {
    ts.trim_end_matches('Z').parse::<u64>().map_err(|_| {
        error::DecapodError::ValidationError(format!("Invalid epoch timestamp: {}", ts))
    })
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "knowledge",
        "version": "0.2.0",
        "description": "Repository context and rationale with merge/supersede lifecycle",
        "commands": [
            {
                "name": "add",
                "description": "Add a knowledge entry (with optional merge/supersede)",
                "parameters": [
                    {"name": "id", "required": true, "description": "Unique knowledge entry ID (ULID or UUID)"},
                    {"name": "title", "required": true, "description": "Short, specific title for the entry"},
                    {"name": "text", "required": true, "description": "Main content/markdown body of the knowledge entry"},
                    {"name": "provenance", "required": true, "description": "Source reference (file:|url:|cmd:|commit:|event: format required)"},
                    {"name": "claim_id", "required": false, "description": "Optional claim ID this knowledge relates to"},
                    {"name": "merge_key", "required": false, "description": "Deduplication key for merge/supersede"},
                    {"name": "on_conflict", "required": false, "description": "Conflict policy: merge|supersede|reject (default: merge)"},
                    {"name": "status", "required": false, "description": "Entry status: active|superseded|deprecated|stale (default: active)"},
                    {"name": "ttl_policy", "required": false, "description": "TTL policy: ephemeral|decay|persistent (default: persistent)"},
                    {"name": "expires_ts", "required": false, "description": "Expiry timestamp (epoch seconds + Z suffix)"}
                ]
            },
            {
                "name": "search",
                "description": "Search knowledge entries with temporal filtering",
                "parameters": [
                    {"name": "query", "required": true, "description": "Search query for title, content, or provenance"},
                    {"name": "as_of", "required": false, "description": "Temporal cutoff (epoch seconds + Z)"},
                    {"name": "window_days", "required": false, "description": "Recency window in days"},
                    {"name": "rank", "required": false, "description": "Ranking mode: relevance|recency_decay (default: relevance)"}
                ]
            },
            {
                "name": "retrieval-log",
                "description": "Log retrieval feedback event",
                "parameters": [
                    {"name": "query", "required": true},
                    {"name": "returned_ids", "required": true},
                    {"name": "used_ids", "required": true},
                    {"name": "outcome", "required": true, "description": "helped|neutral|hurt|unknown"},
                    {"name": "source", "required": false, "description": "invocation|manual_feedback"}
                ]
            },
            {
                "name": "decay",
                "description": "Run decay/prune on expired entries",
                "parameters": [
                    {"name": "policy", "required": false, "description": "Decay policy name"},
                    {"name": "as_of", "required": false, "description": "Reference timestamp"},
                    {"name": "dry_run", "required": false, "description": "Preview without mutating"}
                ]
            },
            {
                "name": "promote",
                "description": "Record a promotion firewall event to procedural knowledge class",
                "parameters": [
                    {"name": "source_entry_id", "required": true},
                    {"name": "evidence_ref", "required": true, "description": "Repeatable; at least one evidence reference required"},
                    {"name": "approved_by", "required": true, "description": "Human approver identifier"},
                    {"name": "reason", "required": true, "description": "Promotion rationale"}
                ]
            }
        ],
        "storage": [
            "knowledge.db",
            "knowledge.retrieval.events.jsonl",
            "knowledge.decay.events.jsonl",
            "knowledge.promotions.jsonl"
        ]
    })
}
