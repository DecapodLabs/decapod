use crate::core::broker::DbBroker;
use crate::core::db::{OptionalExtension, params};
use crate::core::error;
use crate::core::events;
use crate::core::store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeContext {
    pub schema_version: String,
    pub status: String,
    pub records: Vec<KnowledgeContextRecord>,
    pub conflicts: Vec<Vec<String>>,
    pub excluded: BTreeMap<String, usize>,
    pub content_bytes: usize,
    pub source_hash: String,
    pub valid_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeFeedbackSummary {
    pub helped: usize,
    pub neutral: usize,
    pub hurt: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeContextRecord {
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub scope: String,
    pub status: String,
    pub provenance: Vec<String>,
    pub provenance_class: String,
    pub freshness: String,
    pub authority: String,
    pub source_hash: String,
    pub reasons: Vec<String>,
    pub source_observation: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_summary: Option<KnowledgeFeedbackSummary>,
}

fn context_json(value: &impl Serialize) -> Result<Vec<u8>, error::DecapodError> {
    serde_json::to_vec(value).map_err(|e| {
        error::DecapodError::ValidationError(format!("knowledge context serialization: {e}"))
    })
}

/// Resolve advisory knowledge at the repository backend boundary.
/// Current plan/policy authority remains in its canonical sibling surfaces.
pub fn resolve_context(
    store: &Store,
    repo_root: &Path,
    query: Option<&str>,
    task_id: Option<&str>,
    touched_paths: &[String],
    as_of: Option<&str>,
) -> Result<KnowledgeContext, error::DecapodError> {
    let mut packet = KnowledgeContext {
        schema_version: "decapod.knowledge-context.v1".into(),
        status: "no_relevant_advisory_knowledge".into(),
        ..Default::default()
    };
    if store.kind != crate::core::store::StoreKind::Repo {
        packet.status = "private_store_excluded".into();
        return Ok(packet);
    }
    // A worktree can select Cloud, but it cannot downgrade an accepted Cloud
    // repository. Check the invocation's project before consulting the store
    // path, which points at the main checkout even from a linked worktree.
    if crate::core::cloud_workspace::is_cloud_project(repo_root) {
        packet.status = "cloud_knowledge_unavailable".into();
        return Ok(packet);
    }
    if !knowledge_db_path(&store.root).is_file() {
        packet.status = "store_uninitialized".into();
        return Ok(packet);
    }
    let reference = parse_epoch_z(&as_of.map(str::to_owned).unwrap_or_else(now_iso))?;
    let mut scopes = BTreeSet::from(["root".to_string(), "repo".to_string()]);
    if let Some(task) = task_id {
        let scope = format!("task:{task}");
        validate_scope(&scope)?;
        scopes.insert(scope);
    }
    for path in touched_paths.iter().take(64) {
        let scope = format!("path:{path}");
        if validate_scope(&scope).is_ok() {
            let mut current = Path::new(path);
            loop {
                scopes.insert(format!("path:{}", current.to_string_lossy()));
                let Some(parent) = current.parent().filter(|p| !p.as_os_str().is_empty()) else {
                    break;
                };
                current = parent;
            }
        }
    }
    let (entries, typed_available, knowledge_available, write_attestations, feedback_by_entry) =
        DbBroker::new(&store.root).with_conn(
            &knowledge_db_path(&store.root),
            "decapod",
            None,
            "knowledge.context",
            |conn| {
                let table_exists = |name: &str| {
                    conn.query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                        [name],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|count| count == 1)
                };
                let typed_available = table_exists("nodes")?;
        let (attestations, feedback) = if table_exists("events")? {
            read_attestations_and_feedback_on_conn(conn)?
        } else {
            (BTreeMap::new(), BTreeMap::new())
        };
                if !table_exists("knowledge")? {
                    return Ok((Vec::new(), typed_available, false, attestations, feedback));
                }
                let placeholders = (1..=scopes.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT id, title, content, provenance, claim_id, created_at, updated_at,
            status, merge_key, ttl_policy, expires_ts, supersedes_id, scope FROM knowledge
            WHERE scope IN ({placeholders}) AND status IN ('active', 'disputed') ORDER BY id LIMIT 257"
                );
                let mut statement = conn.prepare(&sql)?;
                let rows = statement.query_map(
                    crate::core::db::params_from_iter(scopes.iter()),
                    |row| Ok((entry_from_row(row)?, row.get::<_, String>(12)?)),
                )?;
                Ok((
                    rows.collect::<crate::core::db::Result<Vec<_>>>()?,
                    typed_available,
                    true,
                    attestations,
                    feedback,
                ))
            },
        )?;
    if !knowledge_available {
        packet
            .excluded
            .insert("knowledge_store_uninitialized".into(), 1);
    }
    if entries.len() > 256 {
        packet.status = "candidate_budget_exceeded".into();
        return Ok(packet);
    }
    let mut candidates = Vec::new();
    let mut keys = BTreeMap::<String, Vec<(String, String)>>::new();
    let mut superseded_targets = BTreeSet::<String>::new();
    for (entry, scope) in entries {
        if entry.id.len() > 256
            || entry.title.len() > 512
            || entry.content.len() > 65536
            || entry.provenance.len() > 1024
            || [&entry.id, &entry.title, &entry.content, &entry.provenance]
                .into_iter()
                .any(|text| crate::core::gatekeeper::contains_secret_material(text))
        {
            *packet
                .excluded
                .entry("unsafe_or_oversized_record".into())
                .or_default() += 1;
            continue;
        }
        if entry
            .expires_ts
            .as_deref()
            .is_some_and(|ts| !parse_epoch_z(ts).is_ok_and(|expiry| expiry > reference))
        {
            *packet
                .excluded
                .entry("expired_or_invalid_expiry".into())
                .or_default() += 1;
            continue;
        }
        if let Some(expiry) = &entry.expires_ts
            && packet.valid_until.as_deref().is_none_or(|old| {
                parse_epoch_z(old).unwrap_or(0) > parse_epoch_z(expiry).unwrap_or(0)
            })
        {
            packet.valid_until = Some(expiry.clone());
        }
        let canon_scope = canonical_conflict_scope(&scope);
        if let Some(key) = &entry.merge_key
            && !key.is_empty()
        {
            keys.entry(format!("legacy:{canon_scope}:{key}"))
                .or_default()
                .push((entry.id.clone(), entry.content.clone()));
        } else if !entry.title.trim().is_empty() {
            keys.entry(format!(
                "legacy_title:{canon_scope}:{}",
                entry.title.trim().to_lowercase()
            ))
            .or_default()
            .push((entry.id.clone(), entry.content.clone()));
        }
        if let Some(claim) = entry
            .claim_id
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            keys.entry(format!(
                "legacy_claim:{canon_scope}:{}",
                claim.to_lowercase()
            ))
            .or_default()
            .push((entry.id.clone(), entry.content.clone()));
        }
        let supersedes_id = entry.supersedes_id.clone().filter(|s| !s.is_empty());
        if let Some(ref target) = supersedes_id {
            superseded_targets.insert(target.clone());
        }
        let source_hash = format!("{:x}", Sha256::digest(context_json(&entry)?));
        let entry_feedback = feedback_by_entry.get(&entry.id).cloned();
        let mut reasons = vec!["applicable_repository_scope".into()];
        if entry_feedback.as_ref().is_some_and(|fb| fb.hurt > 0) {
            reasons.push("prior_retrieval_outcome_hurt".into());
        }
        candidates.push(KnowledgeContextRecord {
            id: entry.id,
            title: entry.title,
            excerpt: entry.content,
            scope,
            status: entry.status,
            provenance: vec![entry.provenance],
            provenance_class: "recorded_source_unverified".into(),
            freshness: "not_captured_at_write".into(),
            authority: "advisory_below_current_intent_and_policy".into(),
            source_hash,
            reasons,
            source_observation: BTreeMap::new(),
            supersedes_id,
            feedback_summary: entry_feedback,
        });
    }
    // Typed decisions/preferences/lessons remain owned by federation. A confidence
    // label is declared provenance, not authenticated human policy.
    let (typed, typed_supersedes) = if typed_available {
        (
            crate::plugins::federation_ext::context_nodes(&store.root, &scopes)?,
            crate::plugins::federation_ext::context_supersedes_map(&store.root)?,
        )
    } else {
        packet
            .excluded
            .insert("typed_store_uninitialized".into(), 1);
        (Vec::new(), BTreeMap::new())
    };
    if typed.len() > 256 {
        packet.status = "candidate_budget_exceeded".into();
        return Ok(packet);
    }
    for node in typed {
        let sources = node.sources.as_deref().unwrap_or_default();
        if node.id.len() > 240
            || node.title.len() > 512
            || node.body.len() > 65536
            || sources.iter().map(String::len).sum::<usize>() > 1024
            || [&node.id, &node.title, &node.body]
                .into_iter()
                .chain(sources.iter())
                .any(|text| crate::core::gatekeeper::contains_secret_material(text))
        {
            *packet
                .excluded
                .entry("unsafe_or_oversized_record".into())
                .or_default() += 1;
            continue;
        }
        if !matches!(
            node.node_type.as_str(),
            "decision" | "preference" | "lesson"
        ) || !matches!(node.status.as_str(), "active" | "disputed")
            || !scopes.contains(&node.scope)
        {
            continue;
        }
        // Nonempty effective bounds use the canonical federation time parser's
        // RFC3339 contract; malformed or ended records never become operative.
        let effective = |value: &str| {
            crate::core::time::parse_rfc3339(value)
                .ok()
                .map(|date| date.unix_timestamp())
        };
        if node
            .effective_from
            .as_deref()
            .is_some_and(|ts| effective(ts).is_none_or(|t| t > reference as i64))
            || node
                .effective_to
                .as_deref()
                .is_some_and(|ts| effective(ts).is_none_or(|t| t <= reference as i64))
        {
            *packet
                .excluded
                .entry("outside_effective_window".into())
                .or_default() += 1;
            continue;
        }
        let id = format!("federation:{}", node.id);
        let typed_supersedes_id = typed_supersedes
            .get(&node.id)
            .map(|target| format!("federation:{target}"));
        if let Some(ref target) = typed_supersedes_id {
            superseded_targets.insert(target.clone());
        }
        keys.entry(format!(
            "typed:{}:{}:{}",
            canonical_conflict_scope(&node.scope),
            node.node_type,
            node.title.trim().to_lowercase()
        ))
        .or_default()
        .push((id.clone(), node.body.clone()));
        let source_hash = format!("{:x}", Sha256::digest(context_json(&node)?));
        candidates.push(KnowledgeContextRecord {
            id,
            title: node.title,
            excerpt: node.body,
            scope: node.scope,
            status: node.status,
            provenance: node.sources.unwrap_or_default(),
            provenance_class: format!("declared_{}", node.confidence),
            freshness: "not_captured_at_write".into(),
            authority: "advisory_below_current_intent_and_policy".into(),
            source_hash,
            reasons: vec!["applicable_typed_knowledge".into()],
            source_observation: BTreeMap::new(),
            supersedes_id: typed_supersedes_id,
            feedback_summary: None,
        });
    }
    let mut conflict_ids = BTreeSet::new();
    for entries in keys.values() {
        let active_entries: Vec<&(String, String)> = entries
            .iter()
            .filter(|(id, _)| !superseded_targets.contains(id))
            .collect();
        if active_entries
            .iter()
            .map(|(_, body)| body)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            let mut ids: Vec<_> = active_entries.iter().map(|(id, _)| id.clone()).collect();
            ids.sort();
            ids.dedup();
            conflict_ids.extend(ids.clone());
            if !packet.conflicts.contains(&ids) {
                if packet.conflicts.len() < 8 {
                    if ids.len() > 8 {
                        *packet
                            .excluded
                            .entry("conflict_references_budget".into())
                            .or_default() += ids.len() - 8;
                        ids.truncate(8);
                    }
                    packet.conflicts.push(ids);
                } else {
                    *packet
                        .excluded
                        .entry("conflict_groups_budget".into())
                        .or_default() += 1;
                }
            }
        }
    }
    let bounded_query: String = query.unwrap_or("").chars().take(4096).collect();
    let terms: BTreeSet<_> = bounded_query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.len() >= 3 && term.len() <= 64)
        .map(str::to_lowercase)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(32)
        .collect();
    candidates.sort_by(|a, b| {
        let a_hurt = a.feedback_summary.as_ref().is_some_and(|fb| fb.hurt > 0);
        let b_hurt = b.feedback_summary.as_ref().is_some_and(|fb| fb.hurt > 0);
        let a_net = a
            .feedback_summary
            .as_ref()
            .map_or(0i64, |fb| fb.helped as i64 - fb.hurt as i64);
        let b_net = b
            .feedback_summary
            .as_ref()
            .map_or(0i64, |fb| fb.helped as i64 - fb.hurt as i64);
        a_hurt
            .cmp(&b_hurt)
            .then_with(|| b_net.cmp(&a_net))
            .then_with(|| a.id.cmp(&b.id))
    });
    for mut record in candidates {
        let excluded = if record.status == "disputed" || conflict_ids.contains(&record.id) {
            Some("unresolved_conflict")
        } else if record
            .feedback_summary
            .as_ref()
            .is_some_and(|fb| fb.hurt >= 2 && fb.hurt > fb.helped)
        {
            Some("negative_retrieval_bias")
        } else if record.id.len() > 256
            || record.title.len() > 512
            || record.provenance.iter().map(String::len).sum::<usize>() > 1024
        {
            Some("metadata_budget_exceeded")
        } else if [&record.id, &record.title, &record.excerpt]
            .into_iter()
            .chain(record.provenance.iter())
            .any(|text| crate::core::gatekeeper::contains_secret_material(text))
        {
            Some("credential_material")
        } else if record.provenance.is_empty() {
            Some("unavailable_provenance")
        } else if record
            .provenance
            .iter()
            .any(|source| !is_valid_provenance(source))
        {
            Some("malformed_provenance")
        } else {
            None
        };
        if let Some(reason) = excluded {
            *packet.excluded.entry(reason.into()).or_default() += 1;
            continue;
        }
        let attested_sources = write_attestations.get(&record.id);
        for source in &record.provenance {
            if let Some(file) = source.strip_prefix("file:") {
                let file = file.split('#').next().unwrap_or("");
                let observed = observe_source(repo_root, file);
                if observed == "unavailable" {
                    record.freshness = "source_unavailable".into();
                } else if let Some(attested_sha) = attested_sources.and_then(|map| map.get(source))
                {
                    record.provenance_class = "write_time_source_attested".into();
                    if attested_sha == &observed {
                        if record.freshness == "not_captured_at_write" {
                            record.freshness = "source_attested_current".into();
                        }
                    } else {
                        record.freshness = "source_changed_since_write".into();
                        if !record
                            .reasons
                            .iter()
                            .any(|r| r == "source_file_modified_since_attestation")
                        {
                            record
                                .reasons
                                .push("source_file_modified_since_attestation".into());
                        }
                    }
                }
                record.source_observation.insert(source.clone(), observed);
            }
        }
        let words: BTreeSet<_> = format!("{} {}", record.title, record.excerpt)
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            .collect();
        if !terms.is_empty() && terms.is_disjoint(&words) {
            *packet.excluded.entry("irrelevant".into()).or_default() += 1;
            continue;
        }
        // Unversioned provenance is visible; current file bytes cannot establish
        // what was present when an older assertion was recorded.
        if record.provenance.iter().any(|source| {
            source.starts_with("file:")
                && !repo_root
                    .join(
                        source
                            .trim_start_matches("file:")
                            .split('#')
                            .next()
                            .unwrap_or(""),
                    )
                    .is_file()
        }) {
            record.freshness = "source_unavailable".into();
        }
        record.excerpt = record
            .excerpt
            .chars()
            .scan(0usize, |bytes, ch| {
                *bytes += ch.len_utf8();
                (*bytes <= 512).then_some(ch)
            })
            .collect();
        if packet.records.len() >= 8 || packet.content_bytes + record.excerpt.len() > 4096 {
            *packet
                .excluded
                .entry("output_budget_exceeded".into())
                .or_default() += 1;
            continue;
        }
        packet.content_bytes += record.excerpt.len();
        packet.records.push(record);
    }
    if !packet.conflicts.is_empty() {
        packet.status = "unresolved_conflict".into();
    } else if !packet.records.is_empty() {
        packet.status = "advisory_knowledge_selected".into();
    }
    packet.source_hash = format!("{:x}", Sha256::digest(context_json(&packet)?));
    Ok(packet)
}

fn observe_source(repo_root: &Path, relative: &str) -> String {
    if relative.is_empty()
        || !Path::new(relative)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return "unavailable".into();
    }
    let observed = (|| {
        let root = repo_root.canonicalize().ok()?;
        let file = root.join(relative).canonicalize().ok()?;
        if !file.starts_with(&root) || !file.is_file() || file.metadata().ok()?.len() > 1024 * 1024
        {
            return None;
        }
        Some(format!(
            "sha256:{:x}",
            Sha256::digest(std::fs::read(file).ok()?)
        ))
    })();
    observed.unwrap_or_else(|| "unavailable".into())
}

type WriteAttestationMap = BTreeMap<String, BTreeMap<String, String>>;
type FeedbackSummaryMap = BTreeMap<String, KnowledgeFeedbackSummary>;

fn read_attestations_and_feedback_on_conn(
    conn: &crate::core::db::Connection,
) -> Result<(WriteAttestationMap, FeedbackSummaryMap), error::DecapodError> {
    let mut attestations: WriteAttestationMap = BTreeMap::new();
    let mut feedback: FeedbackSummaryMap = BTreeMap::new();
    let mut last_seq = 0u64;
    let mut last_event_id = String::new();
    loop {
        let mut stmt = conn.prepare(
            "SELECT seq, event_id, ts, event_type, actor, subject_kind, subject_id, payload
             FROM events
             WHERE stream = 'knowledge'
               AND (seq > ?1 OR (seq = ?1 AND event_id > ?2))
             ORDER BY seq ASC, event_id ASC
             LIMIT 512",
        )?;
        let rows = stmt.query_map(params![last_seq, last_event_id], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        let batch = rows.collect::<Result<Vec<_>, _>>()?;
        if batch.is_empty() {
            break;
        }
        let last = batch.last().expect("non-empty batch");
        last_seq = last.0;
        last_event_id = last.1.clone();
        for (_seq, event_id, ts, ev_type, actor, subject_kind, subject_id, raw) in batch {
            let Ok(val) = events::reconstruct_stored_payload(
                events::KNOWLEDGE,
                &event_id,
                &ts,
                &ev_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &raw,
            ) else {
                continue;
            };
            let effective_type = if ev_type.is_empty() {
                val.get("event_type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
            } else {
                ev_type.as_str()
            };
            if effective_type == "knowledge.write_attestation" {
                if let (Some(entry_id), Some(prov), Some(sha)) = (
                    val.get("entry_id").and_then(serde_json::Value::as_str),
                    val.get("provenance").and_then(serde_json::Value::as_str),
                    val.get("observed_source_sha256")
                        .and_then(serde_json::Value::as_str),
                ) && sha != "unavailable"
                {
                    attestations
                        .entry(entry_id.to_string())
                        .or_default()
                        .insert(prov.to_string(), sha.to_string());
                }
                continue;
            }
            if let (Some(outcome), Some(ids)) = (
                val.get("outcome").and_then(serde_json::Value::as_str),
                val.get("used_ids")
                    .and_then(serde_json::Value::as_array)
                    .filter(|arr| !arr.is_empty())
                    .or_else(|| {
                        val.get("returned_ids")
                            .and_then(serde_json::Value::as_array)
                    }),
            ) {
                for id_val in ids {
                    if let Some(id) = id_val.as_str() {
                        let summary = feedback.entry(id.to_string()).or_default();
                        match outcome {
                            "helped" => summary.helped += 1,
                            "neutral" => summary.neutral += 1,
                            "hurt" => summary.hurt += 1,
                            _ => summary.unknown += 1,
                        }
                    }
                }
            }
        }
    }
    Ok((attestations, feedback))
}

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
            "Invalid conflict policy '{other}'. Expected merge|supersede|reject"
        ))),
    }
}

pub fn knowledge_db_path(root: &Path) -> PathBuf {
    root.join(crate::core::schemas::LOCAL_DB_NAME)
}

pub fn add_knowledge(
    store: &Store,
    args: AddKnowledgeParams<'_>,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    add_scoped_knowledge(store, args, "root")
}

/// Add a scoped knowledge entry and bind an append-only write-time source
/// attestation event (`knowledge.write_attestation`) for `file:` provenance
/// against `repo_root` so later `resolve_context` calls can distinguish
/// `source_attested_current` from `source_changed_since_write` (#657).
pub fn add_attested_knowledge(
    store: &Store,
    repo_root: &Path,
    args: AddKnowledgeParams<'_>,
    scope: &str,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    let result = add_scoped_knowledge(store, args.clone(), scope)?;
    let observed_source_sha256 = if let Some(file) = args.provenance.strip_prefix("file:") {
        observe_source(repo_root, file.split('#').next().unwrap_or(""))
    } else {
        "unavailable".to_string()
    };
    let content_sha256 = format!("sha256:{:x}", Sha256::digest(args.content.as_bytes()));
    events::append(
        &store.root,
        events::KNOWLEDGE,
        &serde_json::json!({
            "event_id": crate::core::ulid::new_ulid(),
            "ts": now_iso(),
            "event_type": "knowledge.write_attestation",
            "entry_id": result.id,
            "scope": scope,
            "provenance": args.provenance,
            "content_sha256": content_sha256,
            "observed_source_sha256": observed_source_sha256,
        }),
    )?;
    Ok(result)
}

fn canonical_conflict_scope(scope: &str) -> &str {
    match scope {
        "root" | "repo" => "repo_wide",
        other => other,
    }
}

fn is_valid_provenance(provenance: &str) -> bool {
    if provenance
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    use fancy_regex::Regex;
    let prov_re = Regex::new(
        r"^(file:[^#]+(#L\d+(-L\d+)?)?|url:[^ ]+|cmd:[^ ]+|commit:[a-f0-9]+|event:[A-Z0-9_]+)$",
    )
    .unwrap();
    if !prov_re.is_match(provenance).unwrap_or(false) {
        return false;
    }
    if let Some(url) = provenance.strip_prefix("url:") {
        return url.starts_with("https://")
            || url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost");
    }
    if let Some(file) = provenance.strip_prefix("file:") {
        let rel = file.split('#').next().unwrap_or("");
        return !rel.is_empty()
            && Path::new(rel)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)));
    }
    true
}

/// Scope is an applicability label, never an authorization or promotion claim.
pub fn add_scoped_knowledge(
    store: &Store,
    args: AddKnowledgeParams<'_>,
    scope: &str,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    add_scoped_knowledge_inner(store, args, scope, None)
}

/// Explicitly record a corrected knowledge entry that supersedes `target_id`
/// in the same transaction, even without a shared `merge_key` (#657).
pub fn supersede_knowledge(
    store: &Store,
    args: AddKnowledgeParams<'_>,
    scope: &str,
    target_id: &str,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    add_scoped_knowledge_inner(store, args, scope, Some(target_id))
}

/// Add a scoped knowledge entry that supersedes `target_id` and records
/// write-time source attestation (`knowledge.write_attestation`) (#657).
pub fn add_attested_superseding_knowledge(
    store: &Store,
    repo_root: &Path,
    args: AddKnowledgeParams<'_>,
    scope: &str,
    target_id: &str,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    let result = supersede_knowledge(store, args.clone(), scope, target_id)?;
    let observed_source_sha256 = if let Some(file) = args.provenance.strip_prefix("file:") {
        observe_source(repo_root, file.split('#').next().unwrap_or(""))
    } else {
        "unavailable".to_string()
    };
    let content_sha256 = format!("sha256:{:x}", Sha256::digest(args.content.as_bytes()));
    events::append(
        &store.root,
        events::KNOWLEDGE,
        &serde_json::json!({
            "event_id": crate::core::ulid::new_ulid(),
            "ts": now_iso(),
            "event_type": "knowledge.write_attestation",
            "entry_id": result.id,
            "scope": scope,
            "provenance": args.provenance,
            "content_sha256": content_sha256,
            "observed_source_sha256": observed_source_sha256,
        }),
    )?;
    Ok(result)
}

fn add_scoped_knowledge_inner(
    store: &Store,
    args: AddKnowledgeParams<'_>,
    scope: &str,
    supersedes_target: Option<&str>,
) -> Result<AddKnowledgeResult, error::DecapodError> {
    crate::core::storage_contracts::validate_knowledge_write(
        args.id,
        args.title,
        args.content,
        args.provenance,
        scope,
        args.claim_id,
        args.merge_key,
        args.status,
        args.ttl_policy,
        args.expires_ts,
    )?;
    validate_scope(scope)?;
    if let Some(target_id) = supersedes_target {
        if target_id.trim().is_empty() {
            return Err(error::DecapodError::ValidationError(
                "supersedes target id cannot be empty".into(),
            ));
        }
        if target_id == args.id {
            return Err(error::DecapodError::ValidationError(
                "knowledge entry cannot supersede itself".into(),
            ));
        }
    }
    if [args.id, args.title, args.content, args.provenance]
        .iter()
        .any(|text| crate::core::gatekeeper::contains_secret_material(text))
    {
        return Err(error::DecapodError::ValidationError(
            "knowledge contains credential material".into(),
        ));
    }
    if !is_valid_provenance(args.provenance) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid provenance format: '{}'. Must match scheme (file:|url:|cmd:|commit:|event:)",
            args.provenance
        )));
    }

    if !matches!(
        args.status,
        "active" | "superseded" | "deprecated" | "stale" | "rejected" | "disputed"
    ) {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid knowledge status '{}'. Expected active|superseded|deprecated|stale|rejected|disputed",
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

    if is_procedural_entry_id(args.id) {
        let event_id = args.provenance.strip_prefix("event:").ok_or_else(|| {
            error::DecapodError::ValidationError(
                "procedural knowledge entries require provenance in format `event:<promotion_event_id>`"
                    .to_string(),
            )
        })?;
        let event = lookup_promotion_event(store, event_id)?.ok_or_else(|| {
            error::DecapodError::ValidationError(format!(
                "missing promotion firewall event '{event_id}' in decapod.db:events (stream=knowledge)"
            ))
        })?;
        if event.target_class != "procedural" {
            return Err(error::DecapodError::ValidationError(format!(
                "promotion event '{}' target_class is '{}' (expected procedural)",
                event_id, event.target_class
            )));
        }
        if event.evidence_refs.is_empty() {
            return Err(error::DecapodError::ValidationError(format!(
                "promotion event '{event_id}' is missing evidence_refs"
            )));
        }
        if event.approved_by.trim().is_empty() {
            return Err(error::DecapodError::ValidationError(format!(
                "promotion event '{event_id}' is missing approved_by"
            )));
        }
        ensure_promotion_source_active(store, &event.source_entry_id)?;
    }

    let broker = DbBroker::new(&store.root);
    let db_path = knowledge_db_path(&store.root);
    let now = now_iso();

    broker.with_transaction(&db_path, "decapod", None, "knowledge.add", |conn| {
        let mut action = "inserted".to_string();
        let mut effective_id = args.id.to_string();
        let mut superseded_ids = Vec::new();

        if let Some(target_id) = supersedes_target {
            let previous = read_knowledge_on_conn(conn, target_id)?.ok_or_else(|| {
                error::DecapodError::NotFound(format!(
                    "superseded knowledge entry '{target_id}' not found"
                ))
            })?;
            if !matches!(previous.status.as_str(), "active" | "disputed") {
                return Err(error::DecapodError::ValidationError(format!(
                    "cannot supersede knowledge entry '{target_id}' with status '{}'",
                    previous.status
                )));
            }
            events::append_on_conn(
                conn,
                events::KNOWLEDGE,
                &serde_json::json!({
                    "event_id": crate::core::ulid::new_ulid(),
                    "ts": now,
                    "event_type": "knowledge.before_correction",
                    "entry": previous,
                    "scope": scope,
                    "replacement_id": args.id,
                }),
            )?;
            conn.execute(
                "UPDATE knowledge SET status = 'superseded', updated_at = ?2 WHERE id = ?1",
                params![target_id, now],
            )?;
            superseded_ids.push(target_id.to_string());
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
                    scope,
                    args.status,
                    args.merge_key.unwrap_or(""),
                    Some(target_id),
                    args.ttl_policy,
                    args.expires_ts
                ],
            )?;
            action = "superseded".to_string();
        } else if let Some(merge_key) = args.merge_key {
            let existing = conn.query_row(
                "SELECT id FROM knowledge WHERE merge_key = ?1 AND status = 'active' AND scope = ?2",
                params![merge_key, scope],
                |row| row.get::<_, String>(0),
            ).optional()?;

            if let Some(existing_id) = existing {
                let previous = read_knowledge_on_conn(conn, &existing_id)?;
                events::append_on_conn(conn, events::KNOWLEDGE, &serde_json::json!({
                    "event_id": crate::core::ulid::new_ulid(), "ts": now,
                    "event_type": "knowledge.before_correction", "entry": previous,
                    "scope": scope, "replacement_id": args.id,
                }))?;
                match args.conflict_policy {
                    KnowledgeConflictPolicy::Merge => {
                        conn.execute(
                            "UPDATE knowledge
                             SET title = ?2, content = ?3, provenance = ?4, claim_id = ?5,
                                 ttl_policy = ?6, expires_ts = ?7, updated_at = ?8, status = ?9
                             WHERE id = ?1",
                            params![
                                existing_id,
                                args.title,
                                args.content,
                                args.provenance,
                                args.claim_id,
                                args.ttl_policy,
                                args.expires_ts,
                                now,
                                args.status
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
                                scope,
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
                        scope,
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
                    scope,
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
               AND status = 'active' AND scope IN ('root', 'repo') ORDER BY id",
        )?;
        let q = format!("%{query}%");
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

    // Expiry is a read boundary; manual decay is maintenance, not a prerequisite.
    let reference = options
        .as_of
        .map(parse_epoch_z)
        .transpose()?
        .unwrap_or_else(|| {
            crate::core::time::now_epoch_z()
                .trim_end_matches('Z')
                .parse()
                .unwrap_or(0)
        });
    rows.retain(|entry| {
        entry
            .expires_ts
            .as_deref()
            .is_none_or(|ts| parse_epoch_z(ts).is_ok_and(|expiry| expiry > reference))
    });

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
        let now_secs = reference as f64;
        for entry in &mut rows {
            let created_secs = entry
                .created_at
                .trim_end_matches('Z')
                .parse::<f64>()
                .unwrap_or(0.0);
            let age_days = (now_secs - created_secs).max(0.0) / 86400.0;
            entry.recency_score = Some(1.0 / (1.0 + age_days));
        }
        rows.sort_by(|a, b| {
            b.recency_score
                .unwrap_or(0.0)
                .partial_cmp(&a.recency_score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    Ok(rows)
}

fn validate_scope(scope: &str) -> Result<(), error::DecapodError> {
    let valid = matches!(scope, "root" | "repo" | "private")
        || scope.strip_prefix("task:").is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        })
        || scope.strip_prefix("path:").is_some_and(|path| {
            !path.is_empty()
                && path.len() <= 1024
                && Path::new(path).components().count() <= 32
                && !Path::new(path).is_absolute()
                && Path::new(path)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
        });
    if valid {
        Ok(())
    } else {
        Err(error::DecapodError::ValidationError(
            "invalid knowledge scope".into(),
        ))
    }
}

fn entry_from_row(row: &crate::core::db::Row<'_>) -> crate::core::db::Result<KnowledgeEntry> {
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
}

fn read_knowledge_on_conn(
    conn: &crate::core::db::Connection,
    id: &str,
) -> Result<Option<KnowledgeEntry>, error::DecapodError> {
    Ok(conn
        .query_row(
            "SELECT id, title, content, provenance, claim_id, created_at, updated_at,
        status, merge_key, ttl_policy, expires_ts, supersedes_id FROM knowledge WHERE id = ?1",
            params![id],
            entry_from_row,
        )
        .optional()?)
}

/// Explicit history lookup retains superseded/rejected bodies outside live context.
pub fn get_knowledge(
    store: &Store,
    id: &str,
) -> Result<Option<KnowledgeEntry>, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &knowledge_db_path(&store.root),
        "decapod",
        None,
        "knowledge.get",
        |conn| read_knowledge_on_conn(conn, id),
    )
}

/// Explicit bounded audit, never part of the operative context projection.
pub fn knowledge_history(
    store: &Store,
    id: &str,
) -> Result<Vec<serde_json::Value>, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &knowledge_db_path(&store.root),
        "decapod",
        None,
        "knowledge.history",
        |conn| {
            let mut statement = conn.prepare(
                "SELECT event_id, ts, event_type, actor, subject_kind, subject_id, payload
                 FROM events WHERE stream = 'knowledge'
                   AND event_type = 'knowledge.before_correction'
                   AND json_extract(payload, '$.entry.id') = ?1
                 ORDER BY seq, event_id LIMIT 65",
            )?;
            let rows = statement.query_map([id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?;
            let mut history = Vec::new();
            let mut bytes = 0usize;
            for row in rows {
                let (event_id, ts, event_type, actor, subject_kind, subject_id, raw) = row?;
                bytes += raw.len();
                if history.len() >= 64 || bytes > 1024 * 1024 {
                    return Err(error::DecapodError::ValidationError(
                        "knowledge history exceeds bounded audit; preserve existing journal for explicit recovery"
                            .into(),
                    ));
                }
                let val = events::reconstruct_stored_payload(
                    events::KNOWLEDGE,
                    &event_id,
                    &ts,
                    &event_type,
                    &actor,
                    subject_kind.as_deref(),
                    subject_id.as_deref(),
                    &raw,
                )
                .map_err(|e| {
                    error::DecapodError::ValidationError(format!("invalid knowledge history: {e}"))
                })?;
                history.push(val);
            }
            Ok(history)
        },
    )
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
            "Invalid retrieval outcome '{outcome}'. Expected helped|neutral|hurt|unknown"
        )));
    }
    if !matches!(source, "invocation" | "manual_feedback") {
        return Err(error::DecapodError::ValidationError(format!(
            "Invalid retrieval source '{source}'. Expected invocation|manual_feedback"
        )));
    }

    let event_id = crate::core::ulid::new_ulid();
    let event = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "query": query,
        "returned_ids": returned_ids,
        "used_ids": used_ids,
        "outcome": outcome,
        "source": source,
    });

    events::append(&store.root, events::KNOWLEDGE, &event)?;

    Ok(RetrievalFeedbackResult {
        event_id,
        file: format!(
            "decapod.db:{}",
            events::table_for_stream(events::KNOWLEDGE).unwrap()
        ),
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
    let event_id = crate::core::ulid::new_ulid();
    let event = serde_json::json!({
        "event_id": event_id,
        "ts": now_iso(),
        "policy": policy,
        "as_of": as_of,
        "dry_run": dry_run,
        "stale_ids": stale_ids,
    });

    events::append(&store.root, events::KNOWLEDGE, &event)?;

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

    let source_entry_id = input.source_entry_id.trim();
    crate::core::storage_contracts::validate_knowledge_promotion(
        source_entry_id,
        &evidence_refs,
        input.approved_by.trim(),
        input.actor.trim(),
        input.reason.trim(),
    )?;
    ensure_promotion_source_active(store, source_entry_id)?;

    let event = KnowledgePromotionEvent {
        event_id: crate::core::ulid::new_ulid(),
        ts: now_iso(),
        source_entry_id: source_entry_id.to_string(),
        target_class: "procedural".to_string(),
        evidence_refs,
        approved_by: input.approved_by.trim().to_string(),
        actor: input.actor.trim().to_string(),
        reason: input.reason.trim().to_string(),
    };

    events::append(
        &store.root,
        events::KNOWLEDGE,
        &serde_json::to_value(&event)
            .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
    )?;

    Ok(event)
}

fn ensure_promotion_source_active(
    store: &Store,
    source_entry_id: &str,
) -> Result<(), error::DecapodError> {
    let db_path = knowledge_db_path(&store.root);
    if !db_path.exists() {
        return Ok(());
    }
    let existing = DbBroker::new(&store.root).with_conn(
        &db_path,
        "decapod",
        None,
        "knowledge.get",
        |conn| {
            let has_knowledge = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .map(|c| c == 1)?;
            if !has_knowledge {
                return Ok(None);
            }
            read_knowledge_on_conn(conn, source_entry_id)
        },
    )?;
    if let Some(entry) = existing {
        if entry.status != "active" {
            return Err(error::DecapodError::ValidationError(format!(
                "PROMOTION_FIREWALL_BLOCKED: cannot promote non-active knowledge entry '{}' (status={})",
                source_entry_id, entry.status
            )));
        }
        let now_secs = parse_epoch_z(&now_iso()).unwrap_or(0);
        if let Some(expires_ts) = entry.expires_ts.as_deref()
            && !parse_epoch_z(expires_ts).is_ok_and(|exp| exp > now_secs)
        {
            return Err(error::DecapodError::ValidationError(format!(
                "PROMOTION_FIREWALL_BLOCKED: cannot promote expired knowledge entry '{}'",
                source_entry_id
            )));
        }
    }
    Ok(())
}

fn is_procedural_entry_id(id: &str) -> bool {
    id.starts_with("procedural/")
}

fn lookup_promotion_event(
    store: &Store,
    event_id: &str,
) -> Result<Option<KnowledgePromotionEvent>, error::DecapodError> {
    let db_path = events::canonical_db_path(&store.root);
    if !db_path.exists() {
        return Ok(None);
    }
    let broker = DbBroker::new(&store.root);
    let stored = broker.with_conn(&db_path, "decapod", None, "knowledge.get", |conn| {
        let row = conn
            .query_row(
                "SELECT ts, event_type, actor, subject_kind, subject_id, payload
                 FROM events WHERE stream = 'knowledge' AND event_id = ?1",
                [event_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(error::DecapodError::StorageError)?;
        row.map(|(ts, event_type, actor, subject_kind, subject_id, raw)| {
            events::reconstruct_stored_payload(
                events::KNOWLEDGE,
                event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &raw,
            )
        })
        .transpose()
    })?;
    stored
        .map(|value| {
            serde_json::from_value(value).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "invalid knowledge promotion event in decapod.db:events (stream=knowledge): {e}"
                ))
            })
        })
        .transpose()
}

fn parse_epoch_z(ts: &str) -> Result<u64, error::DecapodError> {
    ts.trim_end_matches('Z')
        .parse::<u64>()
        .map_err(|_| error::DecapodError::ValidationError(format!("Invalid epoch timestamp: {ts}")))
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
                    {"name": "supersedes", "required": false, "description": "Explicit target knowledge entry ID superseded by this correction"},
                    {"name": "scope", "required": false, "description": "Applicability scope: root|repo|task:<id>|path:<rel> (default: root)"},
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
            "decapod.db",
            "decapod.db:events"
        ]
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/plugins/knowledge_tests.rs"]
mod tests;
