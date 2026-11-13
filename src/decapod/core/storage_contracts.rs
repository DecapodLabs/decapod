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
    validate_json_str_bounds("proof_plan", proof_plan, MAX_PROOF_PLAN_BYTES, MAX_JSON_DEPTH)?;
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
            "owners_per_task": MAX_OWNERS_PER_TASK
        },
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
}
