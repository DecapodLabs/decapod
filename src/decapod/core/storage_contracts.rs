//! Versioned logical field, collection, JSON, and authority-convergence contracts
//! for Decapod's local SQLite and hosted Dactyl/PostgreSQL datastores (#1630).
//!
//! All string bounds are measured in **UTF-8 bytes** (`str::len()`), never Unicode
//! scalar or grapheme counts, so Rust write admission, SQLite `length(CAST(x AS BLOB))`,
//! and PostgreSQL `octet_length(x)` agree identically on every input.
//!
//! Write admission fails closed with typed `STORAGE_CONTRACT_VIOLATION` errors and
//! **never silently truncates or coerces** caller-supplied data. Historical rows
//! that predate a bound remain readable and updatable on unrelated fields.

use crate::core::db::{Connection, params};
use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Versioned schema identifier for logical field and collection contracts.
pub const CONTRACT_SCHEMA_VERSION: &str = "decapod.storage-field-contracts.v1";

// ---------------------------------------------------------------------------
// UTF-8 byte bounds by logical field class
// ---------------------------------------------------------------------------

/// Identifiers (`tasks.id`, `events.event_id`, `nodes.id`, `claims.id`, ULIDs).
pub const MAX_ID_BYTES: usize = 128;
/// Digests and content hashes (`sha256:<64hex>`, `tasks.hash`, `record_digest`).
pub const MAX_DIGEST_BYTES: usize = 128;
/// Cooperative or authenticated principal labels (`actor`, `agent_id`, `owner`, `assigned_to`).
pub const MAX_PRINCIPAL_BYTES: usize = 128;
/// Canonical repository identifiers (`org/repo`, `repository`).
pub const MAX_REPO_ID_BYTES: usize = 256;
/// ISO-8601 / epoch-Z timestamps (`ts`, `created_at`, `updated_at`, `due`, `lease_expires_at`).
pub const MAX_TIMESTAMP_BYTES: usize = 64;
/// Event stream and subject-kind discriminators (`stream`, `subject_kind`).
pub const MAX_STREAM_BYTES: usize = 64;
/// Event type / broker operation names (`event_type`, `op`).
pub const MAX_EVENT_TYPE_BYTES: usize = 128;
/// Bounded lifecycle/status/priority/claim-type tokens.
pub const MAX_ENUM_TOKEN_BYTES: usize = 64;
/// Single normalized tag label (`task_tags.tag`).
pub const MAX_TAG_BYTES: usize = 128;
/// Category / component labels (`tasks.category`, `tasks.component`).
pub const MAX_CATEGORY_BYTES: usize = 128;
/// Human-readable titles and short names (`tasks.title`, `nodes.title`, `cron_jobs.name`).
pub const MAX_TITLE_BYTES: usize = 1_024;
/// Paths, external references, and intent anchors (`dir_path`, `ref`, `intent_anchor`).
pub const MAX_PATH_OR_REF_BYTES: usize = 4_096;
/// Task scope expressions (including explicit `paths:...` claim lists).
pub const MAX_SCOPE_BYTES: usize = 16_384;
/// Authored task descriptions, comments, and verification notes.
pub const MAX_DESCRIPTION_BYTES: usize = 65_536;
/// Serialized task proof plans (`task_verification.proof_plan`).
pub const MAX_PROOF_PLAN_BYTES: usize = 65_536;
/// Serialized task verification artifact JSON (`task_verification.verification_artifacts`).
pub const MAX_VERIFICATION_ARTIFACTS_BYTES: usize = 524_288;
/// Knowledge/federation/summary body prose (`knowledge.content`, `nodes.body`, `summaries.summary_text`).
pub const MAX_BODY_PROSE_BYTES: usize = 262_144;
/// Serialized canonical event payload admitted at `events::append_on_conn`.
/// Matches `MAX_SUBJECT_RECORD_BYTES` (8 MiB) ceiling only for recovery fixtures;
/// new runtime event payloads are bounded at 1 MiB unless carrying a reviewed transfer bundle.
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 1_048_576;
/// Reviewed standalone task-transfer export/import event envelope bound (bundle <= 4 MiB + wrapper).
pub const MAX_TRANSFER_EVENT_PAYLOAD_BYTES: usize = 4 * 1024 * 1024 + 65_536;

// ---------------------------------------------------------------------------
// Collection cardinality & JSON structural bounds
// ---------------------------------------------------------------------------

/// Maximum distinct tags per task (`tasks.tags` / `task_tags`).
pub const MAX_TAGS_PER_TASK: usize = 64;
/// Maximum distinct dependency IDs per task (`tasks.depends_on` / `tasks.blocks` / `task_dependencies`).
pub const MAX_DEPENDENCIES_PER_TASK: usize = 128;
/// Maximum owners per task (`task_owners`).
pub const MAX_OWNERS_PER_TASK: usize = 32;
/// Maximum nesting depth for embedded JSON values admitted on write.
pub const MAX_JSON_DEPTH: usize = 32;

fn violation(field: &str, detail: impl AsRef<str>) -> DecapodError {
    DecapodError::ValidationError(format!(
        "STORAGE_CONTRACT_VIOLATION: field '{field}' {}",
        detail.as_ref()
    ))
}

/// Validate a string field against NUL bytes, optional non-empty requirement, and UTF-8 byte bound.
pub fn validate_utf8_field(
    field: &str,
    value: &str,
    max_bytes: usize,
    require_non_empty: bool,
) -> Result<(), DecapodError> {
    if value.as_bytes().contains(&0) {
        return Err(violation(field, "contains forbidden NUL (0x00) byte"));
    }
    if require_non_empty && value.trim().is_empty() {
        return Err(violation(field, "must not be empty or whitespace-only"));
    }
    let actual = value.len();
    if actual > max_bytes {
        return Err(violation(
            field,
            format!(
                "exceeds UTF-8 byte bound ({actual} bytes > {max_bytes} max bytes; {} chars)",
                value.chars().count()
            ),
        ));
    }
    Ok(())
}

/// Validate an optional string field when present.
pub fn validate_optional_utf8_field(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
    require_non_empty_when_some: bool,
) -> Result<(), DecapodError> {
    if let Some(v) = value {
        validate_utf8_field(field, v, max_bytes, require_non_empty_when_some)?;
    }
    Ok(())
}

/// Check JSON nesting depth and NUL-freedom across keys and string leaves.
pub fn validate_json_value_bounds(
    field: &str,
    value: &Value,
    max_bytes: usize,
    max_depth: usize,
) -> Result<usize, DecapodError> {
    fn walk(field: &str, v: &Value, depth: usize, max_depth: usize) -> Result<(), DecapodError> {
        if depth > max_depth {
            return Err(violation(
                field,
                format!("exceeds maximum JSON nesting depth ({depth} > {max_depth})"),
            ));
        }
        match v {
            Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
            Value::String(s) => {
                if s.as_bytes().contains(&0) {
                    return Err(violation(
                        field,
                        "contains forbidden NUL (0x00) byte in JSON string",
                    ));
                }
                Ok(())
            }
            Value::Array(items) => {
                for item in items {
                    walk(field, item, depth + 1, max_depth)?;
                }
                Ok(())
            }
            Value::Object(map) => {
                for (k, val) in map {
                    if k.as_bytes().contains(&0) {
                        return Err(violation(
                            field,
                            "contains forbidden NUL (0x00) byte in JSON object key",
                        ));
                    }
                    walk(field, val, depth + 1, max_depth)?;
                }
                Ok(())
            }
        }
    }

    walk(field, value, 0, max_depth)?;
    let encoded =
        serde_json::to_vec(value).map_err(|e| violation(field, format!("invalid JSON: {e}")))?;
    if encoded.len() > max_bytes {
        return Err(violation(
            field,
            format!(
                "exceeds serialized JSON byte bound ({} bytes > {max_bytes} max bytes)",
                encoded.len()
            ),
        ));
    }
    Ok(encoded.len())
}

/// Validate a serialized JSON string column against UTF-8 byte bound, valid JSON syntax, and depth.
pub fn validate_json_str_bounds(
    field: &str,
    raw: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<(), DecapodError> {
    validate_utf8_field(field, raw, max_bytes, true)?;
    let parsed: Value =
        serde_json::from_str(raw).map_err(|e| violation(field, format!("malformed JSON: {e}")))?;
    validate_json_value_bounds(field, &parsed, max_bytes, max_depth)?;
    Ok(())
}

/// Parse, validate, and normalize a comma-separated tag list into canonical sorted unique tags
/// and their normalized CSV representation (`tasks.tags` <-> `task_tags` convergence).
pub fn normalize_and_validate_tags_csv(raw: &str) -> Result<(Vec<String>, String), DecapodError> {
    if raw.as_bytes().contains(&0) {
        return Err(violation("tags", "contains forbidden NUL (0x00) byte"));
    }
    let mut unique = BTreeSet::new();
    for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        validate_utf8_field("tags[]", part, MAX_TAG_BYTES, true)?;
        unique.insert(part.to_owned());
        if unique.len() > MAX_TAGS_PER_TASK {
            return Err(violation(
                "tags",
                format!(
                    "exceeds tag collection cardinality bound (> {MAX_TAGS_PER_TASK} distinct tags)"
                ),
            ));
        }
    }
    let list: Vec<String> = unique.into_iter().collect();
    let csv = list.join(",");
    Ok((list, csv))
}

/// Parse a comma-separated tag string without enforcing write-time bounds (for historical audits).
pub fn parse_tags_set_lossless(raw: &str) -> BTreeSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Parse, validate, and normalize a comma-separated task dependency/blocks list, preserving
/// deterministic first-seen order without duplicates.
pub fn normalize_and_validate_dependency_csv(
    field: &str,
    raw: &str,
) -> Result<(Vec<String>, String), DecapodError> {
    if raw.as_bytes().contains(&0) {
        return Err(violation(field, "contains forbidden NUL (0x00) byte"));
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        validate_utf8_field(field, part, MAX_ID_BYTES, true)?;
        if seen.insert(part.to_owned()) {
            out.push(part.to_owned());
            if out.len() > MAX_DEPENDENCIES_PER_TASK {
                return Err(violation(
                    field,
                    format!(
                        "exceeds dependency collection cardinality bound (> {MAX_DEPENDENCIES_PER_TASK} task IDs)"
                    ),
                ));
            }
        }
    }
    let csv = out.join(",");
    Ok((out, csv))
}

/// Validate an owner list (`task_owners` / `owner` CSV).
pub fn validate_owner_list(owners: &[String]) -> Result<(), DecapodError> {
    if owners.len() > MAX_OWNERS_PER_TASK {
        return Err(violation(
            "owners",
            format!(
                "exceeds owner collection cardinality bound ({} > {MAX_OWNERS_PER_TASK})",
                owners.len()
            ),
        ));
    }
    for owner in owners {
        validate_utf8_field("owners[]", owner, MAX_PRINCIPAL_BYTES, true)?;
    }
    Ok(())
}

/// Borrowed task creation fields checked at write admission before any SQL mutation.
pub struct TaskWriteInput<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub priority: &'a str,
    pub tags: &'a str,
    pub owners: &'a [String],
    pub due: Option<&'a str>,
    pub r#ref: &'a str,
    pub scope: &'a str,
    pub dir_path: &'a str,
    pub depends_on: &'a str,
    pub blocks: &'a str,
    pub parent_task_id: Option<&'a str>,
}

/// Normalized collections produced by [`validate_task_write`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTaskCollections {
    pub tags: Vec<String>,
    pub tags_csv: String,
    pub depends_on: Vec<String>,
    pub depends_on_csv: String,
    pub blocks: Vec<String>,
    pub blocks_csv: String,
}

/// Validate all fields and collections for `todo add` write admission.
pub fn validate_task_write(
    input: &TaskWriteInput<'_>,
) -> Result<ValidatedTaskCollections, DecapodError> {
    validate_utf8_field("title", input.title, MAX_TITLE_BYTES, true)?;
    validate_utf8_field(
        "description",
        input.description,
        MAX_DESCRIPTION_BYTES,
        false,
    )?;
    validate_utf8_field("priority", input.priority, MAX_ENUM_TOKEN_BYTES, false)?;
    validate_owner_list(input.owners)?;
    validate_optional_utf8_field("due", input.due, MAX_TIMESTAMP_BYTES, false)?;
    validate_utf8_field("ref", input.r#ref, MAX_PATH_OR_REF_BYTES, false)?;
    validate_utf8_field("scope", input.scope, MAX_SCOPE_BYTES, true)?;
    validate_utf8_field("dir_path", input.dir_path, MAX_PATH_OR_REF_BYTES, true)?;
    validate_optional_utf8_field("parent_task_id", input.parent_task_id, MAX_ID_BYTES, true)?;

    let (tags, tags_csv) = normalize_and_validate_tags_csv(input.tags)?;
    let (depends_on, depends_on_csv) =
        normalize_and_validate_dependency_csv("depends_on", input.depends_on)?;
    let (blocks, blocks_csv) = normalize_and_validate_dependency_csv("blocks", input.blocks)?;

    Ok(ValidatedTaskCollections {
        tags,
        tags_csv,
        depends_on,
        depends_on_csv,
        blocks,
        blocks_csv,
    })
}

/// Borrowed task edit fields checked at write admission. Unmodified `None` fields
/// are intentionally untouched so historical oversized fields on unrelated columns
/// remain readable and updatable without forced truncation.
pub struct TaskPatchInput<'a> {
    pub id: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub owners: Option<&'a [String]>,
    pub category: Option<&'a str>,
}

pub fn validate_task_patch(input: &TaskPatchInput<'_>) -> Result<(), DecapodError> {
    validate_utf8_field("id", input.id, MAX_ID_BYTES, true)?;
    validate_optional_utf8_field("title", input.title, MAX_TITLE_BYTES, true)?;
    validate_optional_utf8_field(
        "description",
        input.description,
        MAX_DESCRIPTION_BYTES,
        false,
    )?;
    if let Some(owners) = input.owners {
        validate_owner_list(owners)?;
    }
    validate_optional_utf8_field("category", input.category, MAX_CATEGORY_BYTES, false)?;
    Ok(())
}

pub fn validate_task_comment(
    task_id: &str,
    actor: &str,
    comment: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("task_id", task_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, true)?;
    validate_utf8_field("comment", comment, MAX_DESCRIPTION_BYTES, true)?;
    Ok(())
}

pub fn validate_verification_write(
    todo_id: &str,
    proof_plan: &str,
    verification_artifacts: Option<&str>,
    last_verified_status: Option<&str>,
    last_verified_notes: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("todo_id", todo_id, MAX_ID_BYTES, true)?;
    validate_json_str_bounds(
        "proof_plan",
        proof_plan,
        MAX_PROOF_PLAN_BYTES,
        MAX_JSON_DEPTH,
    )?;
    if let Some(artifacts) = verification_artifacts {
        validate_json_str_bounds(
            "verification_artifacts",
            artifacts,
            MAX_VERIFICATION_ARTIFACTS_BYTES,
            MAX_JSON_DEPTH,
        )?;
    }
    validate_optional_utf8_field(
        "last_verified_status",
        last_verified_status,
        MAX_ENUM_TOKEN_BYTES,
        false,
    )?;
    validate_optional_utf8_field(
        "last_verified_notes",
        last_verified_notes,
        MAX_DESCRIPTION_BYTES,
        false,
    )?;
    Ok(())
}

/// Validate canonical event envelope and payload bounds at `events::append_on_conn`.
#[allow(clippy::too_many_arguments)]
pub fn validate_event_append(
    stream: &str,
    event_id: &str,
    ts: &str,
    event_type: &str,
    actor: &str,
    subject_kind: Option<&str>,
    subject_id: Option<&str>,
    event: &Value,
) -> Result<(), DecapodError> {
    validate_utf8_field("stream", stream, MAX_STREAM_BYTES, true)?;
    validate_utf8_field("event_id", event_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("ts", ts, MAX_TIMESTAMP_BYTES, true)?;
    validate_utf8_field("event_type", event_type, MAX_EVENT_TYPE_BYTES, false)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, true)?;
    validate_optional_utf8_field("subject_kind", subject_kind, MAX_STREAM_BYTES, true)?;
    validate_optional_utf8_field("subject_id", subject_id, MAX_ID_BYTES, true)?;

    let payload_ceiling = if matches!(
        event_type,
        "task.transfer.exported" | "task.transfer.imported"
    ) {
        MAX_TRANSFER_EVENT_PAYLOAD_BYTES
    } else {
        MAX_EVENT_PAYLOAD_BYTES
    };
    validate_json_value_bounds("event.payload", event, payload_ceiling, MAX_JSON_DEPTH)?;
    Ok(())
}

/// Maximum sources/evidence refs per persisted non-task record.
pub const MAX_EVIDENCE_REFS_PER_RECORD: usize = MAX_DEPENDENCIES_PER_TASK;

/// Validate knowledge entry write bounds without altering historical reads.
#[allow(clippy::too_many_arguments)]
pub fn validate_knowledge_write(
    id: &str,
    title: &str,
    content: &str,
    provenance: &str,
    scope: &str,
    claim_id: Option<&str>,
    merge_key: Option<&str>,
    status: &str,
    ttl_policy: &str,
    expires_ts: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("id", id, MAX_ID_BYTES, false)?;
    validate_utf8_field("title", title, MAX_TITLE_BYTES, false)?;
    validate_utf8_field("content", content, MAX_BODY_PROSE_BYTES, false)?;
    validate_utf8_field("provenance", provenance, MAX_PATH_OR_REF_BYTES, false)?;
    validate_utf8_field("scope", scope, MAX_SCOPE_BYTES, false)?;
    validate_optional_utf8_field("claim_id", claim_id, MAX_ID_BYTES, false)?;
    validate_optional_utf8_field("merge_key", merge_key, MAX_ID_BYTES, false)?;
    validate_utf8_field("status", status, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("ttl_policy", ttl_policy, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_optional_utf8_field("expires_ts", expires_ts, MAX_TIMESTAMP_BYTES, false)?;
    Ok(())
}

/// Validate knowledge promotion event bounds.
pub fn validate_knowledge_promotion(
    source_entry_id: &str,
    evidence_refs: &[String],
    approved_by: &str,
    actor: &str,
    reason: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("source_entry_id", source_entry_id, MAX_ID_BYTES, true)?;
    if evidence_refs.len() > MAX_EVIDENCE_REFS_PER_RECORD {
        return Err(violation(
            "evidence_refs",
            format!(
                "exceeds evidence reference collection bound ({} > {MAX_EVIDENCE_REFS_PER_RECORD})",
                evidence_refs.len()
            ),
        ));
    }
    for r in evidence_refs {
        validate_utf8_field("evidence_refs[]", r, MAX_PATH_OR_REF_BYTES, true)?;
    }
    validate_utf8_field("approved_by", approved_by, MAX_PRINCIPAL_BYTES, true)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, true)?;
    validate_utf8_field("reason", reason, MAX_DESCRIPTION_BYTES, true)?;
    Ok(())
}

/// Validate federation node write bounds and normalized tags/sources collections.
#[allow(clippy::too_many_arguments)]
pub fn validate_federation_node_write(
    title: &str,
    node_type: &str,
    priority: &str,
    confidence: &str,
    body: &str,
    sources: &[String],
    tags: &str,
    scope: &str,
    effective_from: Option<&str>,
    actor: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("title", title, MAX_TITLE_BYTES, false)?;
    validate_utf8_field("node_type", node_type, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("priority", priority, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("confidence", confidence, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("body", body, MAX_BODY_PROSE_BYTES, false)?;
    if sources.len() > MAX_EVIDENCE_REFS_PER_RECORD {
        return Err(violation(
            "sources",
            format!(
                "exceeds source collection bound ({} > {MAX_EVIDENCE_REFS_PER_RECORD})",
                sources.len()
            ),
        ));
    }
    for src in sources {
        validate_utf8_field("sources[]", src, MAX_PATH_OR_REF_BYTES, true)?;
    }
    let _ = normalize_and_validate_tags_csv(tags)?;
    validate_utf8_field("scope", scope, MAX_SCOPE_BYTES, false)?;
    validate_optional_utf8_field("effective_from", effective_from, MAX_TIMESTAMP_BYTES, false)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, false)?;
    Ok(())
}

/// Validate federation edge write bounds.
pub fn validate_federation_edge_write(
    source_id: &str,
    target_id: &str,
    edge_type: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("source_id", source_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("target_id", target_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("edge_type", edge_type, MAX_ENUM_TOKEN_BYTES, true)?;
    Ok(())
}

/// Validate health claim and proof write bounds.
pub fn validate_health_claim_write(
    id: &str,
    subject: &str,
    kind: &str,
    provenance: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("id", id, MAX_ID_BYTES, false)?;
    validate_utf8_field("subject", subject, MAX_TITLE_BYTES, false)?;
    validate_utf8_field("kind", kind, MAX_ENUM_TOKEN_BYTES, false)?;
    validate_utf8_field("provenance", provenance, MAX_PATH_OR_REF_BYTES, false)?;
    Ok(())
}

pub fn validate_health_proof_write(
    claim_id: &str,
    surface: &str,
    result: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("claim_id", claim_id, MAX_ID_BYTES, false)?;
    validate_utf8_field("surface", surface, MAX_PATH_OR_REF_BYTES, false)?;
    validate_utf8_field("result", result, MAX_ENUM_TOKEN_BYTES, false)?;
    Ok(())
}

/// Validate decide session and decision write bounds.
pub fn validate_decide_session_write(
    tree_id: &str,
    title: &str,
    description: &str,
    actor: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("tree_id", tree_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("title", title, MAX_TITLE_BYTES, false)?;
    validate_utf8_field("description", description, MAX_DESCRIPTION_BYTES, false)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, false)?;
    Ok(())
}

pub fn validate_decide_record_write(
    session_id: &str,
    question_id: &str,
    value: &str,
    rationale: &str,
    actor: &str,
) -> Result<(), DecapodError> {
    validate_utf8_field("session_id", session_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("question_id", question_id, MAX_ID_BYTES, true)?;
    validate_utf8_field("value", value, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("rationale", rationale, MAX_DESCRIPTION_BYTES, false)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, false)?;
    Ok(())
}

/// Validate cron job write/update bounds.
#[allow(clippy::too_many_arguments)]
pub fn validate_cron_write(
    name: Option<&str>,
    description: Option<&str>,
    schedule: Option<&str>,
    command: Option<&str>,
    status: Option<&str>,
    tags: Option<&str>,
    last_run: Option<&str>,
    next_run: Option<&str>,
) -> Result<(), DecapodError> {
    validate_optional_utf8_field("name", name, MAX_TITLE_BYTES, false)?;
    validate_optional_utf8_field("description", description, MAX_DESCRIPTION_BYTES, false)?;
    validate_optional_utf8_field("schedule", schedule, MAX_ENUM_TOKEN_BYTES, false)?;
    validate_optional_utf8_field("command", command, MAX_PATH_OR_REF_BYTES, false)?;
    validate_optional_utf8_field("status", status, MAX_ENUM_TOKEN_BYTES, false)?;
    if let Some(t) = tags {
        let _ = normalize_and_validate_tags_csv(t)?;
    }
    validate_optional_utf8_field("last_run", last_run, MAX_TIMESTAMP_BYTES, false)?;
    validate_optional_utf8_field("next_run", next_run, MAX_TIMESTAMP_BYTES, false)?;
    Ok(())
}

fn validate_optional_reflex_config(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
    max_depth: usize,
) -> Result<(), DecapodError> {
    let Some(cfg) = value else {
        return Ok(());
    };
    validate_utf8_field(field, cfg, max_bytes, false)?;
    if let Ok(parsed) = serde_json::from_str::<Value>(cfg) {
        validate_json_value_bounds(field, &parsed, max_bytes, max_depth)?;
    }
    Ok(())
}

/// Validate reflex write/update bounds (bounded UTF-8 strings or JSON configs).
#[allow(clippy::too_many_arguments)]
pub fn validate_reflex_write(
    name: Option<&str>,
    description: Option<&str>,
    trigger_type: Option<&str>,
    trigger_config: Option<&str>,
    action_type: Option<&str>,
    action_config: Option<&str>,
    status: Option<&str>,
    tags: Option<&str>,
) -> Result<(), DecapodError> {
    validate_optional_utf8_field("name", name, MAX_TITLE_BYTES, false)?;
    validate_optional_utf8_field("description", description, MAX_DESCRIPTION_BYTES, false)?;
    validate_optional_utf8_field("trigger_type", trigger_type, MAX_ENUM_TOKEN_BYTES, false)?;
    validate_optional_reflex_config(
        "trigger_config",
        trigger_config,
        MAX_PROOF_PLAN_BYTES,
        MAX_JSON_DEPTH,
    )?;
    validate_optional_utf8_field("action_type", action_type, MAX_ENUM_TOKEN_BYTES, false)?;
    validate_optional_reflex_config(
        "action_config",
        action_config,
        MAX_PROOF_PLAN_BYTES,
        MAX_JSON_DEPTH,
    )?;
    validate_optional_utf8_field("status", status, MAX_ENUM_TOKEN_BYTES, false)?;
    if let Some(t) = tags {
        let _ = normalize_and_validate_tags_csv(t)?;
    }
    Ok(())
}

/// Validate feedback write bounds.
pub fn validate_feedback_write(
    source: &str,
    text: &str,
    links: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("source", source, MAX_PATH_OR_REF_BYTES, false)?;
    validate_utf8_field("text", text, MAX_DESCRIPTION_BYTES, false)?;
    validate_optional_utf8_field("links", links, MAX_PATH_OR_REF_BYTES, false)?;
    Ok(())
}

/// Validate archive registry write bounds.
pub fn validate_archive_write(id: &str, path: &str) -> Result<(), DecapodError> {
    validate_utf8_field("id", id, MAX_ID_BYTES, true)?;
    validate_utf8_field("path", path, MAX_PATH_OR_REF_BYTES, true)?;
    Ok(())
}

/// Validate aptitude pattern and observation writes.
pub fn validate_aptitude_pattern_write(
    name: &str,
    category: &str,
    regex_pattern: &str,
    preference_category: Option<&str>,
    preference_key: Option<&str>,
    description: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("name", name, MAX_TITLE_BYTES, true)?;
    validate_utf8_field("category", category, MAX_CATEGORY_BYTES, true)?;
    validate_utf8_field("regex_pattern", regex_pattern, MAX_PATH_OR_REF_BYTES, true)?;
    validate_optional_utf8_field(
        "preference_category",
        preference_category,
        MAX_CATEGORY_BYTES,
        false,
    )?;
    validate_optional_utf8_field("preference_key", preference_key, MAX_TITLE_BYTES, false)?;
    validate_optional_utf8_field("description", description, MAX_DESCRIPTION_BYTES, false)?;
    Ok(())
}

pub fn validate_aptitude_observation_write(
    content: &str,
    category: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("content", content, MAX_DESCRIPTION_BYTES, true)?;
    validate_optional_utf8_field("category", category, MAX_CATEGORY_BYTES, false)?;
    Ok(())
}

/// Validate LCM original ingestion fields (envelope payload bounded by `MAX_EVENT_PAYLOAD_BYTES`).
pub fn validate_lcm_ingest_write(
    content: &str,
    kind: &str,
    actor: &str,
    session_id: Option<&str>,
    source: Option<&str>,
) -> Result<(), DecapodError> {
    validate_utf8_field("content", content, MAX_BODY_PROSE_BYTES, false)?;
    validate_utf8_field("kind", kind, MAX_ENUM_TOKEN_BYTES, true)?;
    validate_utf8_field("actor", actor, MAX_PRINCIPAL_BYTES, false)?;
    validate_optional_utf8_field("session_id", session_id, MAX_ID_BYTES, false)?;
    validate_optional_utf8_field("source", source, MAX_PATH_OR_REF_BYTES, false)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Duplicated-authority convergence (`tasks.tags` <-> `task_tags` and
// `tasks.depends_on` <-> `task_dependencies`)
// ---------------------------------------------------------------------------

/// Audit result for duplicated relational/inline authority on `tasks`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityConvergenceReport {
    pub tasks_checked: usize,
    /// Tasks where `parse_tags_set_lossless(tasks.tags) != set(task_tags.tag)`.
    pub tag_set_divergences: Vec<String>,
    /// Tasks where `tasks.tags` was non-canonical CSV ordering/spacing relative to `task_tags`.
    pub noncanonical_tag_csv_tasks: Vec<String>,
    /// Tasks where `task_dependencies` differs from `parse_dependency_ids(tasks.depends_on) ∩ existing_tasks`.
    pub dependency_edge_divergences: Vec<String>,
    /// Tasks whose `tasks.depends_on` names an unresolved (not yet created or external) task ID.
    /// Preserved intentionally in `tasks.depends_on` without violating `task_dependencies` FK integrity.
    pub unresolved_declared_dependencies: BTreeMap<String, Vec<String>>,
}

impl AuthorityConvergenceReport {
    pub fn is_converged(&self) -> bool {
        self.tag_set_divergences.is_empty() && self.dependency_edge_divergences.is_empty()
    }
}

/// Audit `tasks.tags` <-> `task_tags` and `tasks.depends_on` <-> `task_dependencies`
/// without mutating stored data.
///
/// Authority contract:
/// - `tasks.tags` (authored CSV, normalized to sorted unique tags on write) and
///   `task_tags` (relational WITHOUT ROWID index) MUST represent the exact same tag set.
/// - `tasks.depends_on` is the canonical authored dependency ID list (preserving
///   unresolved/future task IDs that cannot yet enter FK-constrained `task_dependencies`).
/// - `task_dependencies` is the derived resolved-FK edge projection and MUST equal
///   `{ dep_id ∈ parse_dependency_ids(tasks.depends_on) | dep_id ∈ tasks.id }`.
pub fn audit_task_authority_convergence(
    conn: &Connection,
) -> Result<AuthorityConvergenceReport, DecapodError> {
    if !conn.has_table("tasks")? {
        return Ok(AuthorityConvergenceReport::default());
    }
    let mut existing_ids = BTreeSet::new();
    let mut task_rows = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, COALESCE(tags, ''), COALESCE(depends_on, '') FROM tasks ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, tags_csv, depends_csv) = row?;
            existing_ids.insert(id.clone());
            task_rows.push((id, tags_csv, depends_csv));
        }
    }

    let mut report = AuthorityConvergenceReport {
        tasks_checked: task_rows.len(),
        ..AuthorityConvergenceReport::default()
    };

    let has_tags_table = conn.has_table("task_tags")?;
    let has_deps_table = conn.has_table("task_dependencies")?;

    for (task_id, tags_csv, depends_csv) in task_rows {
        let inline_tags = parse_tags_set_lossless(&tags_csv);
        let canonical_csv = inline_tags.iter().cloned().collect::<Vec<_>>().join(",");
        if tags_csv.trim() != canonical_csv {
            report.noncanonical_tag_csv_tasks.push(task_id.clone());
        }
        if has_tags_table {
            let mut stmt =
                conn.prepare("SELECT tag FROM task_tags WHERE task_id = ?1 ORDER BY tag")?;
            let junction_tags: BTreeSet<String> = stmt
                .query_map(params![&task_id], |r| r.get::<_, String>(0))?
                .collect::<Result<BTreeSet<_>, _>>()?;
            if inline_tags != junction_tags {
                report.tag_set_divergences.push(task_id.clone());
            }
        }

        let declared_deps: Vec<String> = depends_csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let mut expected_resolved = BTreeSet::new();
        let mut unresolved = Vec::new();
        let mut seen_unresolved = BTreeSet::new();
        for dep in declared_deps {
            if existing_ids.contains(&dep) {
                expected_resolved.insert(dep);
            } else if seen_unresolved.insert(dep.clone()) {
                unresolved.push(dep);
            }
        }
        if !unresolved.is_empty() {
            report
                .unresolved_declared_dependencies
                .insert(task_id.clone(), unresolved);
        }
        if has_deps_table {
            let mut stmt = conn.prepare(
                "SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ?1 ORDER BY depends_on_task_id",
            )?;
            let actual_resolved: BTreeSet<String> = stmt
                .query_map(params![&task_id], |r| r.get::<_, String>(0))?
                .collect::<Result<BTreeSet<_>, _>>()?;
            if expected_resolved != actual_resolved {
                report.dependency_edge_divergences.push(task_id);
            }
        }
    }

    Ok(report)
}

/// Export the machine-readable field, collection, JSON, and authority-convergence contract.
pub fn contract_manifest() -> Value {
    json!({
        "schema": CONTRACT_SCHEMA_VERSION,
        "length_unit": "utf8_bytes",
        "nul_byte_policy": "reject_fail_closed",
        "truncation_policy": "never_silent_truncate",
        "legacy_read_policy": "preserve_historical_oversized_values_on_read_and_unrelated_updates",
        "field_classes": {
            "id": { "max_utf8_bytes": MAX_ID_BYTES, "non_empty": true },
            "digest": { "max_utf8_bytes": MAX_DIGEST_BYTES, "non_empty": true },
            "principal": { "max_utf8_bytes": MAX_PRINCIPAL_BYTES, "non_empty": true },
            "repository_id": { "max_utf8_bytes": MAX_REPO_ID_BYTES, "non_empty": true },
            "timestamp": { "max_utf8_bytes": MAX_TIMESTAMP_BYTES, "non_empty": true },
            "stream": { "max_utf8_bytes": MAX_STREAM_BYTES, "non_empty": true },
            "event_type": { "max_utf8_bytes": MAX_EVENT_TYPE_BYTES, "non_empty": false },
            "enum_token": { "max_utf8_bytes": MAX_ENUM_TOKEN_BYTES, "non_empty": false },
            "tag": { "max_utf8_bytes": MAX_TAG_BYTES, "non_empty": true },
            "category_or_component": { "max_utf8_bytes": MAX_CATEGORY_BYTES, "non_empty": false },
            "title": { "max_utf8_bytes": MAX_TITLE_BYTES, "non_empty": true },
            "path_or_ref": { "max_utf8_bytes": MAX_PATH_OR_REF_BYTES, "non_empty": false },
            "scope": { "max_utf8_bytes": MAX_SCOPE_BYTES, "non_empty": true },
            "description_or_notes": { "max_utf8_bytes": MAX_DESCRIPTION_BYTES, "non_empty": false },
            "proof_plan_json": { "max_utf8_bytes": MAX_PROOF_PLAN_BYTES, "max_json_depth": MAX_JSON_DEPTH },
            "verification_artifacts_json": { "max_utf8_bytes": MAX_VERIFICATION_ARTIFACTS_BYTES, "max_json_depth": MAX_JSON_DEPTH },
            "body_prose": { "max_utf8_bytes": MAX_BODY_PROSE_BYTES, "non_empty": false },
            "event_payload_json": { "max_utf8_bytes": MAX_EVENT_PAYLOAD_BYTES, "max_json_depth": MAX_JSON_DEPTH },
            "transfer_event_payload_json": { "max_utf8_bytes": MAX_TRANSFER_EVENT_PAYLOAD_BYTES, "max_json_depth": MAX_JSON_DEPTH }
        },
        "collection_bounds": {
            "tags_per_task": MAX_TAGS_PER_TASK,
            "dependencies_per_task": MAX_DEPENDENCIES_PER_TASK,
            "owners_per_task": MAX_OWNERS_PER_TASK,
            "evidence_refs_per_record": MAX_EVIDENCE_REFS_PER_RECORD
        },
        "sql_boundary_parity": {
            "rust_utf8_len": "str::len(value) in UTF-8 bytes + fail-closed NUL rejection",
            "sqlite_length_function": "length(CAST(col AS BLOB))",
            "postgres_length_function": "octet_length(convert_to(col, 'UTF8'))",
            "null_byte_parity": "PostgreSQL TEXT rejects 0x00 natively; Rust validator enforces identical fail-closed rejection on SQLite/local stores"
        },
        "propodus_abi_v1_reconciliation": {
            "abi_id": "decapod.logical_storage_abi.v1",
            "policy": "decapod local bounds are strictly <= propodus v1 bounds except runtime events.payload which stays 1 MiB locally while cloud/wire adapters bound payloads at 262144 bytes",
            "reconciled_overrides": {
                "tasks.id": { "decapod": MAX_ID_BYTES, "propodus_v1": 512 },
                "tasks.title": { "decapod": MAX_TITLE_BYTES, "propodus_v1": 4096 },
                "tasks.description": { "decapod": MAX_DESCRIPTION_BYTES, "propodus_v1": 262_144 },
                "tasks.scope": { "decapod": MAX_SCOPE_BYTES, "propodus_v1": 65_536 },
                "events.payload": { "decapod": MAX_EVENT_PAYLOAD_BYTES, "propodus_v1": 262_144 }
            }
        },
        "field_table": field_contract_table(),
        "authority_convergence": {
            "tags": {
                "canonical_authored_column": "tasks.tags",
                "derived_relational_projection": "task_tags",
                "write_normalization": "sorted_unique_csv",
                "invariant": "set(parse_csv(tasks.tags)) == set(task_tags.tag)"
            },
            "dependencies": {
                "canonical_authored_column": "tasks.depends_on",
                "derived_relational_projection": "task_dependencies",
                "unresolved_policy": "retained_in_tasks_depends_on_and_backfilled_when_target_task_created",
                "invariant": "set(task_dependencies.depends_on_task_id) == set(parse_csv(tasks.depends_on)) intersect set(tasks.id)"
            }
        }
    })
}

/// Declarative, versioned per-column field & collection contract entry shared across
/// Rust write validators, SQLite write-boundary triggers, and PostgreSQL CHECK parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FieldContractSpec {
    pub table: &'static str,
    pub column: &'static str,
    pub field_class: &'static str,
    pub max_utf8_bytes: usize,
    pub non_empty: bool,
    pub nullable: bool,
    pub max_json_depth: Option<usize>,
    pub max_collection_len: Option<usize>,
    pub propodus_abi_v1_max_bytes: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
const fn fc(
    table: &'static str,
    column: &'static str,
    field_class: &'static str,
    max_utf8_bytes: usize,
    non_empty: bool,
    nullable: bool,
    max_json_depth: Option<usize>,
    max_collection_len: Option<usize>,
    propodus_abi_v1_max_bytes: Option<usize>,
) -> FieldContractSpec {
    FieldContractSpec {
        table,
        column,
        field_class,
        max_utf8_bytes,
        non_empty,
        nullable,
        max_json_depth,
        max_collection_len,
        propodus_abi_v1_max_bytes,
    }
}

const MAX_TAGS_CSV_BYTES: usize = MAX_TAGS_PER_TASK * (MAX_TAG_BYTES + 1);
const MAX_DEPS_CSV_BYTES: usize = MAX_DEPENDENCIES_PER_TASK * (MAX_ID_BYTES + 1);

/// Unified versioned table of active persisted columns across local SQLite and hosted PostgreSQL.
pub const FIELD_CONTRACTS: &[FieldContractSpec] = &[
    // --- tasks ---
    fc(
        "tasks",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "tasks",
        "hash",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        Some(256),
    ),
    fc(
        "tasks",
        "title",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        Some(4_096),
    ),
    fc(
        "tasks",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        Some(262_144),
    ),
    fc(
        "tasks",
        "tags",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        true,
        None,
        Some(MAX_TAGS_PER_TASK),
        Some(8_192),
    ),
    fc(
        "tasks",
        "owner",
        "principal",
        MAX_PRINCIPAL_BYTES,
        false,
        true,
        None,
        Some(MAX_OWNERS_PER_TASK),
        Some(512),
    ),
    fc(
        "tasks",
        "due",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "ref",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        true,
        None,
        None,
        Some(4_096),
    ),
    fc(
        "tasks",
        "status",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "created_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "updated_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "completed_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "closed_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "dir_path",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        Some(4_096),
    ),
    fc(
        "tasks",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        Some(65_536),
    ),
    fc(
        "tasks",
        "parent_task_id",
        "id",
        MAX_ID_BYTES,
        false,
        true,
        None,
        None,
        Some(512),
    ),
    fc(
        "tasks",
        "priority",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        false,
        true,
        None,
        None,
        Some(64),
    ),
    fc(
        "tasks",
        "depends_on",
        "dependency_csv",
        MAX_DEPS_CSV_BYTES,
        false,
        true,
        None,
        Some(MAX_DEPENDENCIES_PER_TASK),
        None,
    ),
    fc(
        "tasks",
        "blocks",
        "dependency_csv",
        MAX_DEPS_CSV_BYTES,
        false,
        true,
        None,
        Some(MAX_DEPENDENCIES_PER_TASK),
        None,
    ),
    fc(
        "tasks",
        "category",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        false,
        true,
        None,
        None,
        Some(256),
    ),
    fc(
        "tasks",
        "component",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "tasks",
        "assigned_to",
        "principal",
        MAX_PRINCIPAL_BYTES,
        false,
        true,
        None,
        None,
        Some(512),
    ),
    fc(
        "tasks",
        "assigned_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "tasks",
        "lease_expires_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "tasks",
        "lease_lifecycle",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "tasks",
        "intent_anchor",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    // --- task_tags, task_owners, task_dependencies ---
    fc(
        "task_tags",
        "task_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_tags",
        "tag",
        "tag",
        MAX_TAG_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "task_owners",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_owners",
        "task_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_owners",
        "agent_id",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_owners",
        "claimed_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "task_owners",
        "claim_type",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "task_dependencies",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_dependencies",
        "task_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_dependencies",
        "depends_on_task_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_dependencies",
        "created_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    // --- task_verification ---
    fc(
        "task_verification",
        "todo_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "task_verification",
        "proof_plan",
        "proof_plan_json",
        MAX_PROOF_PLAN_BYTES,
        true,
        false,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "task_verification",
        "verification_artifacts",
        "verification_artifacts_json",
        MAX_VERIFICATION_ARTIFACTS_BYTES,
        false,
        true,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "task_verification",
        "last_verified_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "task_verification",
        "last_verified_status",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "task_verification",
        "last_verified_notes",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "task_verification",
        "updated_at",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    // --- events & component deduplication ---
    fc(
        "events",
        "event_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "events",
        "ts",
        "timestamp",
        MAX_TIMESTAMP_BYTES,
        true,
        false,
        None,
        None,
        Some(64),
    ),
    fc(
        "events",
        "stream",
        "stream",
        MAX_STREAM_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "events",
        "subject_kind",
        "stream",
        MAX_STREAM_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "events",
        "subject_id",
        "id",
        MAX_ID_BYTES,
        false,
        true,
        None,
        None,
        Some(512),
    ),
    fc(
        "events",
        "event_type",
        "event_type",
        MAX_EVENT_TYPE_BYTES,
        true,
        false,
        None,
        None,
        Some(128),
    ),
    fc(
        "events",
        "payload",
        "event_payload_json",
        MAX_EVENT_PAYLOAD_BYTES,
        true,
        false,
        Some(MAX_JSON_DEPTH),
        None,
        Some(262_144),
    ),
    fc(
        "events",
        "actor",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        Some(512),
    ),
    fc(
        "storage_components",
        "component_key",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_components",
        "component_kind",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_components",
        "content_sha256",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_components",
        "content_text",
        "event_payload_json",
        MAX_EVENT_PAYLOAD_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_component_refs",
        "component_key",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_component_refs",
        "owner_table",
        "stream",
        MAX_STREAM_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_component_refs",
        "owner_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_migration_manifests",
        "manifest_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "storage_migration_manifests",
        "manifest_sha256",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    // --- coordination & authoring ---
    fc(
        "categories",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "categories",
        "name",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "categories",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "categories",
        "keywords",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "risk_zones",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "risk_zones",
        "zone_name",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "risk_zones",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "agents",
        "agent_id",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "authoring_bindings",
        "worktree_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "authoring_bindings",
        "task_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "authoring_bindings",
        "agent_id",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "authoring_bindings",
        "branch",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "authoring_bindings",
        "repository_id",
        "repository_id",
        MAX_REPO_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    // --- non-todo local stores ---
    fc(
        "knowledge",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "title",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "content",
        "body_prose",
        MAX_BODY_PROSE_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "provenance",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "claim_id",
        "id",
        MAX_ID_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "tags",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        false,
        None,
        Some(MAX_TAGS_PER_TASK),
        None,
    ),
    fc(
        "knowledge",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "knowledge",
        "dir_path",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "node_type",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "status",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "priority",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "confidence",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "title",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "body",
        "body_prose",
        MAX_BODY_PROSE_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "tags",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        false,
        None,
        Some(MAX_TAGS_PER_TASK),
        None,
    ),
    fc(
        "nodes",
        "dir_path",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "nodes",
        "actor",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_sources",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_sources",
        "node_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_sources",
        "source",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_edges",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_edges",
        "source_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_edges",
        "target_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_edges",
        "edge_type",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "node_edges",
        "rationale",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "claims",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "claims",
        "subject",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "claims",
        "kind",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "claims",
        "provenance",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "proof_events",
        "event_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "proof_events",
        "claim_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "proof_events",
        "surface",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "proof_events",
        "result",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "health_cache",
        "claim_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "health_cache",
        "computed_state",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "health_cache",
        "reason",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "approvals",
        "approval_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "approvals",
        "action_fingerprint",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "approvals",
        "actor",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "approvals",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligations",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligations",
        "intent_ref",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligations",
        "risk_tier",
        "enum_token",
        MAX_ENUM_TOKEN_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligations",
        "required_proofs",
        "proof_plan_json",
        MAX_PROOF_PLAN_BYTES,
        true,
        false,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "obligations",
        "metadata",
        "verification_artifacts_json",
        MAX_VERIFICATION_ARTIFACTS_BYTES,
        false,
        true,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "obligation_edges",
        "edge_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligation_edges",
        "from_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "obligation_edges",
        "to_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "sessions",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "sessions",
        "tree_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "sessions",
        "title",
        "title",
        MAX_TITLE_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "sessions",
        "actor",
        "principal",
        MAX_PRINCIPAL_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "decisions",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "decisions",
        "session_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "decisions",
        "question",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "decisions",
        "rationale",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "name",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "schedule",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "command",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "tags",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        true,
        None,
        Some(MAX_TAGS_PER_TASK),
        None,
    ),
    fc(
        "cron_jobs",
        "dir_path",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "cron_jobs",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "reflexes",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "reflexes",
        "name",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "reflexes",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "reflexes",
        "trigger_config",
        "proof_plan_json",
        MAX_PROOF_PLAN_BYTES,
        false,
        true,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "reflexes",
        "action_config",
        "proof_plan_json",
        MAX_PROOF_PLAN_BYTES,
        true,
        false,
        Some(MAX_JSON_DEPTH),
        None,
        None,
    ),
    fc(
        "reflexes",
        "tags",
        "tag_csv",
        MAX_TAGS_CSV_BYTES,
        false,
        true,
        None,
        Some(MAX_TAGS_PER_TASK),
        None,
    ),
    fc(
        "feedback",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "feedback",
        "source",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "feedback",
        "text",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "feedback",
        "links",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "archives",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "archives",
        "path",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "archives",
        "content_hash",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "archives",
        "summary_hash",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "preferences",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "preferences",
        "category",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "preferences",
        "key",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "preferences",
        "value",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "patterns",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "patterns",
        "name",
        "title",
        MAX_TITLE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "patterns",
        "category",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "patterns",
        "regex_pattern",
        "path_or_ref",
        MAX_PATH_OR_REF_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "patterns",
        "description",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "observations",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "observations",
        "content",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "observations",
        "category",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "consolidations",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "consolidations",
        "reason",
        "description_or_notes",
        MAX_DESCRIPTION_BYTES,
        false,
        true,
        None,
        None,
        None,
    ),
    fc(
        "agent_prompts",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "agent_prompts",
        "context",
        "category_or_component",
        MAX_CATEGORY_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "agent_prompts",
        "prompt_text",
        "body_prose",
        MAX_BODY_PROSE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "originals_index",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "originals_index",
        "content_sha256",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "originals_index",
        "actor",
        "principal",
        MAX_PRINCIPAL_BYTES,
        false,
        false,
        None,
        None,
        None,
    ),
    fc(
        "summaries",
        "id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "summaries",
        "summary_text",
        "body_prose",
        MAX_BODY_PROSE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "request_dedupe",
        "op_id",
        "id",
        MAX_ID_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "request_dedupe",
        "scope",
        "scope",
        MAX_SCOPE_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
    fc(
        "request_dedupe",
        "payload_sha256",
        "digest",
        MAX_DIGEST_BYTES,
        true,
        false,
        None,
        None,
        None,
    ),
];

/// Return the versioned field & collection contract table across local SQLite and PostgreSQL.
pub fn field_contract_table() -> &'static [FieldContractSpec] {
    FIELD_CONTRACTS
}

/// Render an equivalent PostgreSQL `CHECK` expression enforcing UTF-8 byte limits via
/// `octet_length(convert_to(col, 'UTF8'))` (PostgreSQL natively rejects `0x00` in `TEXT`).
pub fn postgres_check_constraint_sql(spec: &FieldContractSpec) -> String {
    let col = format!("\"{}\"", spec.column);
    let len_expr = format!(
        "octet_length(convert_to({col}, 'UTF8')) <= {}",
        spec.max_utf8_bytes
    );
    if spec.nullable {
        format!("({col} IS NULL OR {len_expr})")
    } else {
        len_expr
    }
}

/// Core `(table, column, max_utf8_bytes)` tuples enforced at the SQLite trigger write boundary.
/// Historical rows violating older bounds remain readable and updatable on unrelated columns.
const SQLITE_TRIGGER_BOUNDS: &[(&str, &str, usize, bool)] = &[
    // tasks: enforce both BEFORE INSERT and BEFORE UPDATE OF <col>
    ("tasks", "id", MAX_ID_BYTES, true),
    ("tasks", "hash", MAX_DIGEST_BYTES, true),
    ("tasks", "title", MAX_TITLE_BYTES, true),
    ("tasks", "description", MAX_DESCRIPTION_BYTES, true),
    ("tasks", "owner", MAX_PRINCIPAL_BYTES, true),
    ("tasks", "ref", MAX_PATH_OR_REF_BYTES, true),
    ("tasks", "dir_path", MAX_PATH_OR_REF_BYTES, true),
    ("tasks", "scope", MAX_SCOPE_BYTES, true),
    ("tasks", "assigned_to", MAX_PRINCIPAL_BYTES, true),
    ("tasks", "intent_anchor", MAX_PATH_OR_REF_BYTES, true),
    // task_verification: enforce both BEFORE INSERT and BEFORE UPDATE OF <cols>
    ("task_verification", "todo_id", MAX_ID_BYTES, true),
    (
        "task_verification",
        "proof_plan",
        MAX_PROOF_PLAN_BYTES,
        true,
    ),
    (
        "task_verification",
        "verification_artifacts",
        MAX_VERIFICATION_ARTIFACTS_BYTES,
        true,
    ),
    (
        "task_verification",
        "last_verified_notes",
        MAX_DESCRIPTION_BYTES,
        true,
    ),
    // events: enforce BEFORE INSERT across bounded columns + BEFORE UPDATE OF metadata;
    // payload update leaves byte ceiling unbound here so read-side corruption tests can stage >8 MiB rows.
    ("events", "event_id", MAX_ID_BYTES, true),
    ("events", "stream", MAX_STREAM_BYTES, true),
    ("events", "event_type", MAX_EVENT_TYPE_BYTES, true),
    ("events", "actor", MAX_PRINCIPAL_BYTES, true),
    ("events", "payload", MAX_TRANSFER_EVENT_PAYLOAD_BYTES, false),
];

fn sqlite_table_columns(conn: &Connection, table: &str) -> Result<BTreeSet<String>, DecapodError> {
    let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let cols = stmt
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(cols)
}

fn sqlite_existing_contract_triggers(conn: &Connection) -> Result<BTreeSet<String>, DecapodError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'trigger' AND name LIKE 'trg_decapod_%'",
    )?;
    let triggers = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(triggers)
}

/// Install idempotent SQLite `BEFORE INSERT` and `BEFORE UPDATE OF <cols>` write-boundary
/// triggers matching `core::storage_contracts` UTF-8 byte & NUL-byte limits without breaking
/// historical oversized rows or unrelated-column updates. Uses one compact trigger pair per table
/// so `sqlite_schema` metadata stays minimal (~1 SQLite page).
pub fn ensure_sqlite_write_boundary_triggers(conn: &Connection) -> Result<(), DecapodError> {
    let existing_triggers = sqlite_existing_contract_triggers(conn)?;
    let mut cols_by_table: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    for table in ["tasks", "task_verification", "events"] {
        cols_by_table.insert(table, sqlite_table_columns(conn, table)?);
    }

    let mut stmts = Vec::new();
    for table in ["tasks", "task_verification", "events"] {
        let Some(cols) = cols_by_table.get(table) else {
            continue;
        };
        let mut ins_terms = Vec::new();
        let mut upd_cols = Vec::new();
        let mut upd_terms = Vec::new();

        for &(_, col, max_bytes, enforce_max_on_update) in SQLITE_TRIGGER_BOUNDS
            .iter()
            .filter(|(t, _, _, _)| *t == table)
        {
            if !cols.contains(col) {
                continue;
            }
            let qcol = if col == "ref" {
                "\"ref\"".to_string()
            } else {
                col.to_string()
            };
            let b = format!("CAST(NEW.{qcol} AS BLOB)");
            let nul_or_len = format!("instr({b},zeroblob(1))>0 OR length({b})>{max_bytes}");
            ins_terms.push(nul_or_len.clone());
            upd_cols.push(qcol.clone());
            let upd_check = if enforce_max_on_update {
                format!("({nul_or_len})")
            } else {
                format!("instr({b},zeroblob(1))>0")
            };
            upd_terms.push(format!("(NEW.{qcol} IS NOT OLD.{qcol} AND {upd_check})"));
        }

        if ins_terms.is_empty() {
            continue;
        }
        let msg = format!("STORAGE_CONTRACT_VIOLATION: {table} write bound");
        let ins_name = format!("trg_decapod_{table}_ins_contract");
        let upd_name = format!("trg_decapod_{table}_upd_contract");
        // Execute each CREATE TRIGGER via `execute` (not `execute_batch`) so Dactyl's
        // statement splitter never splits on `;` inside `BEGIN ... END`.
        if !existing_triggers.contains(&ins_name) {
            stmts.push(format!(
                "CREATE TRIGGER IF NOT EXISTS {ins_name} BEFORE INSERT ON {table} FOR EACH ROW WHEN {} BEGIN SELECT RAISE(ABORT,'{msg}'); END",
                ins_terms.join(" OR ")
            ));
        }
        if !existing_triggers.contains(&upd_name) {
            stmts.push(format!(
                "CREATE TRIGGER IF NOT EXISTS {upd_name} BEFORE UPDATE OF {} ON {table} FOR EACH ROW WHEN {} BEGIN SELECT RAISE(ABORT,'{msg}'); END",
                upd_cols.join(","),
                upd_terms.join(" OR ")
            ));
        }
    }

    for stmt in stmts {
        conn.execute(&stmt, [])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_byte_bound_distinguishes_multibyte_chars_from_byte_count() {
        // 300 4-byte emojis = 300 chars (<= 1024), but 1200 UTF-8 bytes (> MAX_TITLE_BYTES = 1024).
        let multibyte = "🦀".repeat(300);
        assert_eq!(multibyte.chars().count(), 300);
        assert_eq!(multibyte.len(), 1200);
        let err = validate_utf8_field("title", &multibyte, MAX_TITLE_BYTES, true).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("STORAGE_CONTRACT_VIOLATION"), "{msg}");
        assert!(msg.contains("1200 bytes > 1024 max bytes"), "{msg}");
        assert!(msg.contains("300 chars"), "{msg}");

        // Exact boundary (256 * 4 = 1024 bytes) succeeds without truncation.
        let exact = "🦀".repeat(256);
        assert_eq!(exact.len(), MAX_TITLE_BYTES);
        validate_utf8_field("title", &exact, MAX_TITLE_BYTES, true).unwrap();
    }

    #[test]
    fn nul_bytes_and_excess_json_depth_fail_closed() {
        assert!(validate_utf8_field("title", "abc\0def", MAX_TITLE_BYTES, true).is_err());
        let mut nested = json!("leaf");
        for _ in 0..(MAX_JSON_DEPTH + 2) {
            nested = json!({ "next": nested });
        }
        let err =
            validate_json_value_bounds("payload", &nested, MAX_EVENT_PAYLOAD_BYTES, MAX_JSON_DEPTH)
                .unwrap_err();
        assert!(err.to_string().contains("JSON nesting depth"));
    }

    #[test]
    fn tag_and_dependency_collections_normalize_and_enforce_cardinality() {
        let (tags, csv) = normalize_and_validate_tags_csv(" zeta , alpha , zeta ,, beta ").unwrap();
        assert_eq!(tags, vec!["alpha", "beta", "zeta"]);
        assert_eq!(csv, "alpha,beta,zeta");

        let too_many_tags = (0..=MAX_TAGS_PER_TASK)
            .map(|i| format!("tag-{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(normalize_and_validate_tags_csv(&too_many_tags).is_err());

        let too_many_deps = (0..=MAX_DEPENDENCIES_PER_TASK)
            .map(|i| format!("code_{i:016x}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(normalize_and_validate_dependency_csv("depends_on", &too_many_deps).is_err());
    }

    #[test]
    fn field_contract_table_and_sqlite_write_boundary_triggers_match_postgres_sql() {
        let table = field_contract_table();
        assert!(table.iter().any(|s| s.table == "tasks"
            && s.column == "title"
            && s.max_utf8_bytes == MAX_TITLE_BYTES));
        assert!(table.iter().any(|s| s.table == "events"
            && s.column == "payload"
            && s.max_utf8_bytes == MAX_EVENT_PAYLOAD_BYTES));
        assert!(table.iter().any(|s| s.table == "knowledge"
            && s.column == "content"
            && s.max_utf8_bytes == MAX_BODY_PROSE_BYTES));

        let manifest = contract_manifest();
        assert!(
            manifest["field_table"]
                .as_array()
                .is_some_and(|a| a.len() == table.len())
        );
        let title_spec = table
            .iter()
            .find(|s| s.table == "tasks" && s.column == "title")
            .unwrap();
        assert_eq!(
            postgres_check_constraint_sql(title_spec),
            "octet_length(convert_to(\"title\", 'UTF8')) <= 1024"
        );

        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        crate::core::todo::initialize_todo_db(&data).unwrap();
        let conn = Connection::open(data.join(crate::core::schemas::LOCAL_DB_NAME)).unwrap();

        // Direct SQL INSERT violating MAX_TITLE_BYTES on multibyte UTF-8 must fail closed inside SQLite.
        let over_title = "🦀".repeat(257); // 1028 bytes > 1024
        let ins_err = conn
            .execute(
                "INSERT INTO tasks(id, hash, title, status, created_at, updated_at, dir_path, scope)
                 VALUES('code_01testovertitle', '01test', ?1, 'open', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '.', 'repo')",
                params![over_title],
            )
            .unwrap_err();
        assert!(
            ins_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
            "{ins_err}"
        );

        // Direct SQL INSERT containing NUL byte must fail closed inside SQLite.
        let nul_err = conn
            .execute(
                "INSERT INTO tasks(id, hash, title, status, created_at, updated_at, dir_path, scope)
                 VALUES('code_01testnul', '01test', ?1, 'open', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '.', 'repo')",
                params!["bad\0title"],
            )
            .unwrap_err();
        assert!(
            nul_err.to_string().contains("STORAGE_CONTRACT_VIOLATION"),
            "{nul_err}"
        );

        // Exact 1024-byte UTF-8 boundary succeeds cleanly.
        let exact_title = "🦀".repeat(256);
        conn.execute(
            "INSERT INTO tasks(id, hash, title, status, created_at, updated_at, dir_path, scope)
             VALUES('code_01testexact', '01test', ?1, 'open', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '.', 'repo')",
            params![exact_title],
        )
        .unwrap();
    }
}
