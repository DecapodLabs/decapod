//! Transactional, source-pinned physical compaction, forward v2 storage migration,
//! downgrade-refusal guards, lossless historical event/prompt compaction, and
//! reference-aware component garbage collection.
//!
//! Never rename or rewrite a customer table without verifying shape, keys,
//! indexes, exact row content, logical digests, and foreign-key integrity.
//! Physical `VACUUM` page reclamation is never executed automatically on hot CLI
//! paths or implicit startup migrations; it requires an explicit operator call.

use crate::core::db::{Connection, OptionalExtension, params};
use crate::core::error::DecapodError;
use crate::core::events;
use crate::core::schemas;
use crate::core::storage_codec::{self, StoredText};
use dactyl_db::Parameter;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const V2_COMPACTION_MIGRATION_ID: &str = "sqlite.compact_lossless_storage.v002";
pub const STORAGE_FORMAT_VERSION_KEY: &str = "format_version";
pub const STORAGE_MIN_READER_VERSION_KEY: &str = "min_reader_version";
pub const STORAGE_MIN_WRITER_VERSION_KEY: &str = "min_writer_version";

fn unsafe_schema(why: &str) -> DecapodError {
    DecapodError::ValidationError(format!("RELATIONAL_COMPACTION_UNSAFE: {why}"))
}

fn incompatible_format(why: impl AsRef<str>) -> DecapodError {
    DecapodError::ValidationError(format!("STORAGE_FORMAT_INCOMPATIBLE: {}", why.as_ref()))
}

fn parse_strict_semver(version: &str) -> Option<[u64; 3]> {
    let core = version.split('-').next()?.split('+').next()?;
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

/// Fail closed when opening a datastore stamped by a newer incompatible storage
/// format or requiring a newer reader/writer binary version.
pub fn check_storage_format_compatibility(conn: &Connection) -> Result<(), DecapodError> {
    check_storage_format_compatibility_for_version(conn, env!("CARGO_PKG_VERSION"), true)
}

/// Read-only compatibility guard checking `format_version` and `min_reader_version`.
pub fn check_read_compatibility(conn: &Connection) -> Result<(), DecapodError> {
    check_storage_format_compatibility_for_version(conn, env!("CARGO_PKG_VERSION"), false)
}

pub fn check_storage_format_compatibility_for_version(
    conn: &Connection,
    binary_version: &str,
    require_write: bool,
) -> Result<(), DecapodError> {
    if !conn.has_table("meta")? {
        return Ok(());
    }
    let current_semver = parse_strict_semver(binary_version).ok_or_else(|| {
        incompatible_format(format!(
            "invalid evaluating binary semver '{binary_version}'"
        ))
    })?;

    let format_raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace = ?1 AND key = ?2",
            params![
                schemas::STORAGE_FORMAT_META_NAMESPACE,
                STORAGE_FORMAT_VERSION_KEY
            ],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = format_raw {
        let observed = raw.parse::<u32>().map_err(|_| {
            incompatible_format(format!("malformed storage.format_version '{raw}' in meta"))
        })?;
        if observed == 0 || observed > schemas::STORAGE_FORMAT_VERSION {
            return Err(incompatible_format(format!(
                "datastore requires storage format v{observed}, but Decapod {binary_version} supports up to v{}; upgrade Decapod instead of downgrading across compact storage formats",
                schemas::STORAGE_FORMAT_VERSION
            )));
        }
    }

    let min_reader_raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace = ?1 AND key = ?2",
            params![
                schemas::STORAGE_FORMAT_META_NAMESPACE,
                STORAGE_MIN_READER_VERSION_KEY
            ],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(min_reader) = min_reader_raw {
        let required = parse_strict_semver(&min_reader).ok_or_else(|| {
            incompatible_format(format!(
                "malformed storage.min_reader_version '{min_reader}' in meta"
            ))
        })?;
        if current_semver < required {
            return Err(incompatible_format(format!(
                "datastore requires reader >= {min_reader}, found {binary_version}"
            )));
        }
    }

    if require_write {
        let min_writer_raw: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE namespace = ?1 AND key = ?2",
                params![
                    schemas::STORAGE_FORMAT_META_NAMESPACE,
                    STORAGE_MIN_WRITER_VERSION_KEY
                ],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(min_writer) = min_writer_raw {
            let required = parse_strict_semver(&min_writer).ok_or_else(|| {
                incompatible_format(format!(
                    "malformed storage.min_writer_version '{min_writer}' in meta"
                ))
            })?;
            if current_semver < required {
                return Err(incompatible_format(format!(
                    "datastore requires writer >= {min_writer}, found {binary_version}"
                )));
            }
        }
    }

    Ok(())
}

/// Stamp v2 compact storage format metadata in `meta`.
pub fn stamp_storage_format_metadata(conn: &Connection) -> Result<(), DecapodError> {
    conn.execute(schemas::TODO_DB_SCHEMA_META, [])?;
    for (key, val) in [
        (
            STORAGE_FORMAT_VERSION_KEY,
            schemas::STORAGE_FORMAT_VERSION.to_string(),
        ),
        (
            STORAGE_MIN_READER_VERSION_KEY,
            schemas::STORAGE_MIN_READER_VERSION.to_string(),
        ),
        (
            STORAGE_MIN_WRITER_VERSION_KEY,
            schemas::STORAGE_MIN_WRITER_VERSION.to_string(),
        ),
    ] {
        conn.execute(
            "INSERT INTO meta(namespace, key, value) VALUES(?1, ?2, ?3)
             ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value",
            params![schemas::STORAGE_FORMAT_META_NAMESPACE, key, val],
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPromptDedupeReport {
    pub groups_compacted: usize,
    pub duplicate_rows_removed: usize,
    pub surviving_rows: usize,
}

/// Collapse exact duplicate `(context, prompt_text, priority, active)` rows in
/// `agent_prompts` while preserving the earliest `created_at`, total `usage_count`,
/// latest `last_shown_at`, and latest `updated_at`.
pub fn deduplicate_agent_prompts(
    conn: &Connection,
) -> Result<AgentPromptDedupeReport, DecapodError> {
    if !conn.has_table("agent_prompts")? {
        return Ok(AgentPromptDedupeReport::default());
    }

    #[derive(Clone)]
    struct PromptRow {
        id: String,
        context: String,
        prompt_text: String,
        priority: i64,
        active: i64,
        usage_count: i64,
        last_shown_at: Option<String>,
        created_at: String,
        updated_at: Option<String>,
    }

    let mut stmt = conn.prepare(
        "SELECT id, context, prompt_text, priority, active, usage_count, last_shown_at, created_at, updated_at
         FROM agent_prompts
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PromptRow {
            id: r.get(0)?,
            context: r.get(1)?,
            prompt_text: r.get(2)?,
            priority: r.get(3)?,
            active: r.get(4)?,
            usage_count: r.get(5)?,
            last_shown_at: r.get(6)?,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    })?;

    let mut grouped: BTreeMap<(String, String, i64, i64), Vec<PromptRow>> = BTreeMap::new();
    for row in rows {
        let row = row?;
        grouped
            .entry((
                row.context.clone(),
                row.prompt_text.clone(),
                row.priority,
                row.active,
            ))
            .or_default()
            .push(row);
    }

    let mut report = AgentPromptDedupeReport {
        surviving_rows: grouped.len(),
        ..Default::default()
    };

    for (_, group) in grouped {
        if group.len() <= 1 {
            continue;
        }
        report.groups_compacted += 1;
        let survivor = &group[0];
        let min_created = group
            .iter()
            .map(|r| r.created_at.as_str())
            .min()
            .unwrap_or(survivor.created_at.as_str())
            .to_owned();
        let total_usage = group
            .iter()
            .fold(0i64, |acc, r| acc.saturating_add(r.usage_count.max(0)));
        let max_last_shown = group
            .iter()
            .filter_map(|r| r.last_shown_at.as_deref())
            .max()
            .map(str::to_owned);
        let max_updated = group
            .iter()
            .filter_map(|r| r.updated_at.as_deref())
            .max()
            .map(str::to_owned);

        conn.execute(
            "UPDATE agent_prompts
             SET created_at = ?1, usage_count = ?2, last_shown_at = ?3, updated_at = ?4
             WHERE id = ?5",
            params![
                min_created,
                total_usage,
                max_last_shown,
                max_updated,
                survivor.id
            ],
        )?;
        for dup in &group[1..] {
            conn.execute("DELETE FROM agent_prompts WHERE id = ?1", [&dup.id])?;
            report.duplicate_rows_removed += 1;
        }
    }

    Ok(report)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventCompactionReport {
    pub scanned_rows: u64,
    pub compacted_rows: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

fn is_already_compact_event_payload(value: &Value) -> bool {
    events::is_compact_envelope_value(value)
        || value
            .as_object()
            .is_some_and(|obj| obj.len() == 1 && obj.contains_key("_ce_raw"))
}

/// Rewrite legacy non-federation event envelopes into `_ce_v1` compact storage,
/// verifying exact `reconstruct_stored_payload` equivalence on every rewritten row.
pub fn compact_historical_events(conn: &Connection) -> Result<EventCompactionReport, DecapodError> {
    if !conn.has_table("events")? {
        return Ok(EventCompactionReport::default());
    }
    let mut report = EventCompactionReport::default();
    let mut last_seq = 0u64;

    loop {
        let mut stmt = conn.prepare(
            "SELECT seq, stream, event_id, ts, event_type, actor, subject_kind, subject_id, payload
             FROM events
             WHERE stream <> 'federation' AND seq > ?1
             ORDER BY seq ASC
             LIMIT 256",
        )?;
        let rows = stmt.query_map([last_seq], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let batch = rows.collect::<Result<Vec<_>, _>>()?;
        if batch.is_empty() {
            break;
        }
        last_seq = batch.last().expect("non-empty batch").0;

        for (
            _seq,
            stream,
            event_id,
            ts,
            event_type,
            actor,
            subject_kind,
            subject_id,
            raw_payload,
        ) in batch
        {
            report.scanned_rows += 1;
            let raw_len = raw_payload.len() as u64;
            report.bytes_before += raw_len;

            let Ok(parsed) = serde_json::from_str::<Value>(&raw_payload) else {
                report.bytes_after += raw_len;
                continue;
            };
            if is_already_compact_event_payload(&parsed) {
                report.bytes_after += raw_len;
                continue;
            }
            let compact_val = events::compact_envelope_for_storage(
                &stream,
                &event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &parsed,
            );
            let compact_str = serde_json::to_string(&compact_val)
                .map_err(|e| unsafe_schema(&format!("compact serialization failed: {e}")))?;
            if compact_str.len() >= raw_payload.len() {
                report.bytes_after += raw_len;
                continue;
            }
            let roundtrip = events::reconstruct_stored_payload(
                &stream,
                &event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &compact_str,
            )?;
            if roundtrip != parsed {
                return Err(unsafe_schema(
                    "compact event envelope failed exact lossless round-trip check",
                ));
            }
            conn.execute(
                "UPDATE events SET payload = ?1 WHERE stream = ?2 AND event_id = ?3",
                params![compact_str, stream, event_id],
            )?;
            report.compacted_rows += 1;
            report.bytes_after += compact_str.len() as u64;
        }
    }

    Ok(report)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentGcReport {
    pub dry_run: bool,
    pub total_components_scanned: usize,
    pub referenced_components: usize,
    pub unreferenced_components: usize,
    pub deleted_components: usize,
    pub reclaimable_payload_bytes: u64,
}

type ComponentKey = (String, u32, u64, String);

fn collect_event_embedded_component_keys(
    conn: &Connection,
    reachable: &mut BTreeSet<ComponentKey>,
) -> Result<(), DecapodError> {
    if !conn.has_table("events")? {
        return Ok(());
    }
    let mut last_seq = 0u64;
    loop {
        let mut stmt = conn.prepare(
            "SELECT seq, payload FROM events
             WHERE stream = 'todo'
               AND event_type IN ('task.journal.checkpoint', 'task.transfer.exported', 'task.transfer.imported')
               AND seq > ?1
             ORDER BY seq ASC
             LIMIT 256",
        )?;
        let rows = stmt.query_map([last_seq], |r| {
            Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?))
        })?;
        let batch = rows.collect::<Result<Vec<_>, _>>()?;
        if batch.is_empty() {
            break;
        }
        last_seq = batch.last().expect("non-empty batch").0;
        for (_seq, raw) in batch {
            let Ok(val) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            collect_component_keys_from_json(&val, reachable);
        }
    }
    Ok(())
}

fn collect_component_keys_from_json(val: &Value, reachable: &mut BTreeSet<ComponentKey>) {
    match val {
        Value::Object(map) => {
            if let Some(Value::Object(refs)) = map.get("component_refs") {
                for cref in refs.values() {
                    if let (Some(kind), Some(codec), Some(byte_len), Some(sha)) = (
                        cref.get("kind").and_then(Value::as_str),
                        cref.get("codec_version").and_then(Value::as_u64),
                        cref.get("byte_len").and_then(Value::as_u64),
                        cref.get("content_sha256").and_then(Value::as_str),
                    ) && let Ok(codec_u32) = u32::try_from(codec)
                    {
                        reachable.insert((kind.to_owned(), codec_u32, byte_len, sha.to_owned()));
                    }
                }
            }
            if let Some(Value::Array(comps)) = map.get("components") {
                for comp in comps {
                    if let (Some(kind), Some(codec), Some(byte_len), Some(sha)) = (
                        comp.get("kind").and_then(Value::as_str),
                        comp.get("codec_version").and_then(Value::as_u64),
                        comp.get("byte_len").and_then(Value::as_u64),
                        comp.get("content_sha256").and_then(Value::as_str),
                    ) && let Ok(codec_u32) = u32::try_from(codec)
                    {
                        reachable.insert((kind.to_owned(), codec_u32, byte_len, sha.to_owned()));
                    }
                }
            }
            for child in map.values() {
                collect_component_keys_from_json(child, reachable);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_component_keys_from_json(item, reachable);
            }
        }
        _ => {}
    }
}

/// Safe, reference-aware, idempotent garbage collection for `storage_components`.
/// Never deletes a component referenced by `storage_component_refs` or embedded
/// in any historical task checkpoint / transfer provenance event.
pub fn gc_unreferenced_components(
    conn: &Connection,
    dry_run: bool,
) -> Result<ComponentGcReport, DecapodError> {
    if !conn.has_table("storage_components")? {
        return Ok(ComponentGcReport {
            dry_run,
            ..Default::default()
        });
    }

    let started_tx = if conn.is_autocommit() {
        if dry_run {
            conn.execute_batch("BEGIN DEFERRED")?;
        } else {
            conn.execute_batch("BEGIN IMMEDIATE")?;
        }
        true
    } else {
        false
    };

    let result = (|| -> Result<ComponentGcReport, DecapodError> {
        let mut reachable: BTreeSet<ComponentKey> = BTreeSet::new();
        if conn.has_table("storage_component_refs")? {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT component_kind, codec_version, byte_len, content_sha256
                 FROM storage_component_refs",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u64>(1)? as u32,
                    r.get::<_, u64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                reachable.insert(row?);
            }
        }
        collect_event_embedded_component_keys(conn, &mut reachable)?;

        let mut stmt = conn.prepare(
            "SELECT component_kind, codec_version, byte_len, content_sha256, length(stored_bytes)
             FROM storage_components
             ORDER BY component_kind, codec_version, byte_len, content_sha256",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                (
                    r.get::<_, String>(0)?,
                    r.get::<_, u64>(1)? as u32,
                    r.get::<_, u64>(2)?,
                    r.get::<_, String>(3)?,
                ),
                r.get::<_, u64>(4)?,
            ))
        })?;
        let all_components = rows.collect::<Result<Vec<_>, _>>()?;

        let mut report = ComponentGcReport {
            dry_run,
            total_components_scanned: all_components.len(),
            ..Default::default()
        };

        let mut candidates = Vec::new();
        for (key, stored_bytes) in all_components {
            if reachable.contains(&key) {
                report.referenced_components += 1;
            } else {
                report.unreferenced_components += 1;
                report.reclaimable_payload_bytes += stored_bytes;
                candidates.push(key);
            }
        }

        if !dry_run {
            for (kind, codec, byte_len, sha) in &candidates {
                let changed = conn.execute(
                    "DELETE FROM storage_components
                     WHERE component_kind = ?1
                       AND codec_version = ?2
                       AND byte_len = ?3
                       AND content_sha256 = ?4",
                    params![kind, codec, byte_len, sha],
                )?;
                report.deleted_components += changed;
            }
            let fk_violations: i64 =
                conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })?;
            if fk_violations > 0 {
                return Err(unsafe_schema(
                    "component GC triggered foreign-key violation",
                ));
            }
        }

        Ok(report)
    })();

    if started_tx {
        match (&result, dry_run) {
            (Ok(_), false) => conn.execute_batch("COMMIT")?,
            _ => {
                let _ = conn.execute_batch("ROLLBACK");
            }
        }
    }
    result
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExplicitVacuumReport {
    pub page_size: u64,
    pub pages_before: u64,
    pub pages_after: u64,
    pub freelist_before: u64,
    pub freelist_after: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Explicit-only physical page reclamation (`VACUUM`). Never called implicitly
/// on normal CLI read/write paths or automatic startup migrations.
pub fn reclaim_pages_explicit(conn: &Connection) -> Result<ExplicitVacuumReport, DecapodError> {
    if !conn.is_autocommit() {
        return Err(DecapodError::ValidationError(
            "EXPLICIT_VACUUM_REQUIRES_AUTOCOMMIT: physical page reclamation cannot run inside an open transaction"
                .into(),
        ));
    }
    let page_size: u64 =
        conn.query_row("SELECT page_size FROM pragma_page_size", [], |r| r.get(0))?;
    let pages_before: u64 =
        conn.query_row("SELECT page_count FROM pragma_page_count", [], |r| r.get(0))?;
    let freelist_before: u64 = conn.query_row(
        "SELECT freelist_count FROM pragma_freelist_count",
        [],
        |r| r.get(0),
    )?;

    conn.execute_batch("VACUUM")?;

    let pages_after: u64 =
        conn.query_row("SELECT page_count FROM pragma_page_count", [], |r| r.get(0))?;
    let freelist_after: u64 = conn.query_row(
        "SELECT freelist_count FROM pragma_freelist_count",
        [],
        |r| r.get(0),
    )?;

    Ok(ExplicitVacuumReport {
        page_size,
        pages_before,
        pages_after,
        freelist_before,
        freelist_after,
        bytes_before: pages_before.saturating_mul(page_size),
        bytes_after: pages_after.saturating_mul(page_size),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionMigrationManifest {
    pub migration_id: String,
    pub format_version: u32,
    pub pre_events_count: u64,
    pub post_events_count: u64,
    pub events_logical_sha256: String,
    pub tasks_count: u64,
    pub prompt_dedupe: AgentPromptDedupeReport,
    pub event_compaction: EventCompactionReport,
    pub manifest_sha256: String,
}

fn logical_events_inventory(conn: &Connection) -> Result<(u64, String), DecapodError> {
    if !conn.has_table("events")? {
        return Ok((0, format!("{:x}", Sha256::digest(b"empty"))));
    }
    let mut hasher = Sha256::new();
    let mut count = 0u64;
    let mut last_seq = 0u64;
    loop {
        let mut stmt = conn.prepare(
            "SELECT seq, stream, event_id, ts, event_type, actor, subject_kind, subject_id, payload
             FROM events
             WHERE seq > ?1
             ORDER BY seq ASC, event_id ASC
             LIMIT 256",
        )?;
        let rows = stmt.query_map([last_seq], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let batch = rows.collect::<Result<Vec<_>, _>>()?;
        if batch.is_empty() {
            break;
        }
        last_seq = batch.last().expect("non-empty batch").0;
        for (seq, stream, event_id, ts, event_type, actor, subject_kind, subject_id, raw) in batch {
            count += 1;
            let logical = match events::reconstruct_stored_payload(
                &stream,
                &event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &raw,
            ) {
                Ok(v) => serde_json::to_vec(&v).unwrap_or_else(|_| raw.as_bytes().to_vec()),
                Err(_) => raw.as_bytes().to_vec(),
            };
            hasher.update(seq.to_le_bytes());
            hasher.update(stream.as_bytes());
            hasher.update([0]);
            hasher.update(event_id.as_bytes());
            hasher.update([0]);
            hasher.update(&logical);
            hasher.update([0xff]);
        }
    }
    Ok((count, format!("{:x}", hasher.finalize())))
}

fn distinct_agent_prompt_groups(conn: &Connection) -> Result<u64, DecapodError> {
    if !conn.has_table("agent_prompts")? {
        return Ok(0);
    }
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT DISTINCT context, prompt_text, priority, active FROM agent_prompts
         )",
        [],
        |r| r.get(0),
    )?)
}

/// Apply forward checksummed v2 compact storage migration (`1200`) atomically.
/// Preserves every logical event and task projection byte-for-byte; rolls back
/// completely on any invariant mismatch or mid-transaction interruption.
pub fn apply_v2_compaction_migration(
    conn: &Connection,
) -> Result<CompactionMigrationManifest, DecapodError> {
    let started_tx = if conn.is_autocommit() {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        true
    } else {
        false
    };

    let result = (|| -> Result<CompactionMigrationManifest, DecapodError> {
        check_storage_format_compatibility(conn)?;
        let (pre_events_count, pre_events_sha) = logical_events_inventory(conn)?;
        let pre_prompt_groups = distinct_agent_prompt_groups(conn)?;
        let tasks_count: u64 = if conn.has_table("tasks")? {
            conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?
        } else {
            0
        };

        conn.execute_batch(schemas::STORAGE_COMPONENTS_SCHEMA)?;
        conn.execute_batch(schemas::STORAGE_COMPONENT_REFS_SCHEMA)?;
        conn.execute_batch(schemas::STORAGE_MIGRATION_MANIFESTS_SCHEMA)?;

        let prompt_dedupe = deduplicate_agent_prompts(conn)?;
        let event_compaction = compact_historical_events(conn)?;

        let (post_events_count, post_events_sha) = logical_events_inventory(conn)?;
        let post_prompt_groups = distinct_agent_prompt_groups(conn)?;
        if post_events_count != pre_events_count || post_events_sha != pre_events_sha {
            return Err(unsafe_schema(
                "logical event count or SHA-256 digest changed during v2 compaction",
            ));
        }
        if post_prompt_groups != pre_prompt_groups {
            return Err(unsafe_schema(
                "distinct agent_prompts cardinality changed during deduplication",
            ));
        }
        let fk_violations: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if fk_violations > 0 {
            return Err(unsafe_schema(
                "foreign-key integrity failed after v2 storage compaction",
            ));
        }

        stamp_storage_format_metadata(conn)?;

        let summary_json = json!({
            "migration_id": V2_COMPACTION_MIGRATION_ID,
            "format_version": schemas::STORAGE_FORMAT_VERSION,
            "min_reader_version": schemas::STORAGE_MIN_READER_VERSION,
            "min_writer_version": schemas::STORAGE_MIN_WRITER_VERSION,
            "pre_events_count": pre_events_count,
            "post_events_count": post_events_count,
            "events_logical_sha256": post_events_sha,
            "tasks_count": tasks_count,
            "prompt_dedupe": prompt_dedupe,
            "event_compaction": event_compaction,
        });
        let summary_str = serde_json::to_string(&summary_json)
            .map_err(|e| unsafe_schema(&format!("manifest serialization failed: {e}")))?;
        let manifest_sha256 = format!("{:x}", Sha256::digest(summary_str.as_bytes()));
        let applied_at = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO storage_migration_manifests(
                 migration_id, schema_version, applied_at, binary_version, pre_digest, post_digest, manifest_json
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(migration_id) DO UPDATE SET
                 schema_version = excluded.schema_version,
                 applied_at = excluded.applied_at,
                 binary_version = excluded.binary_version,
                 pre_digest = excluded.pre_digest,
                 post_digest = excluded.post_digest,
                 manifest_json = excluded.manifest_json",
            params![
                V2_COMPACTION_MIGRATION_ID,
                schemas::STORAGE_FORMAT_VERSION.to_string(),
                applied_at,
                env!("CARGO_PKG_VERSION"),
                pre_events_sha,
                post_events_sha,
                summary_str,
            ],
        )?;

        Ok(CompactionMigrationManifest {
            migration_id: V2_COMPACTION_MIGRATION_ID.to_owned(),
            format_version: schemas::STORAGE_FORMAT_VERSION,
            pre_events_count,
            post_events_count,
            events_logical_sha256: post_events_sha,
            tasks_count,
            prompt_dedupe,
            event_compaction,
            manifest_sha256,
        })
    })();

    if started_tx {
        match &result {
            Ok(_) => conn.execute_batch("COMMIT")?,
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK");
            }
        }
    }
    result
}

pub fn apply(conn: &Connection) -> Result<(), DecapodError> {
    compact_pair(conn, "meta")?;
    compact_pair(conn, "task_tags")?;
    compact_summaries(conn)?;
    let violations: i64 =
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        return Err(unsafe_schema("foreign-key integrity changed"));
    }
    Ok(())
}

fn compact_pair(conn: &Connection, table: &str) -> Result<(), DecapodError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let (shape, ddl, insert, compare, allowed_index, secondary) = match table {
        "meta" => (
            "namespace:TEXT:1:1|key:TEXT:1:2|value:TEXT:1:0",
            "CREATE TABLE __decapod_compact_meta (namespace TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(namespace,key)) WITHOUT ROWID",
            "INSERT INTO __decapod_compact_meta SELECT namespace,key,value FROM meta",
            "SELECT (SELECT count(*) FROM meta)=(SELECT count(*) FROM __decapod_compact_meta) AND NOT EXISTS(SELECT namespace,key,value FROM meta EXCEPT SELECT namespace,key,value FROM __decapod_compact_meta)",
            "",
            "",
        ),
        "task_tags" => (
            "task_id:TEXT:1:1|tag:TEXT:1:2",
            "CREATE TABLE __decapod_compact_task_tags (task_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(task_id,tag), FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE) WITHOUT ROWID",
            "INSERT INTO __decapod_compact_task_tags SELECT task_id,tag FROM task_tags",
            "SELECT (SELECT count(*) FROM task_tags)=(SELECT count(*) FROM __decapod_compact_task_tags) AND NOT EXISTS(SELECT task_id,tag FROM task_tags EXCEPT SELECT task_id,tag FROM __decapod_compact_task_tags)",
            "idx_task_tags_tag",
            "CREATE INDEX idx_task_tags_tag ON task_tags(tag)",
        ),
        _ => return Err(unsafe_schema("unexpected table requested")),
    };
    let actual: String = conn.query_row(
        "SELECT group_concat(sig,'|') FROM (
           SELECT name||':'||upper(type)||':'||\"notnull\"||':'||pk AS sig
           FROM pragma_table_info(?1) ORDER BY cid)",
        [table],
        |r| r.get(0),
    )?;
    if actual != shape {
        return Err(unsafe_schema(&format!(
            "{table} has altered columns or primary key"
        )));
    }
    let without_rowid: i64 = conn.query_row(
        "SELECT wr FROM pragma_table_list WHERE \"schema\"='main' AND name=?1 AND type='table'",
        [table],
        |r| r.get(0),
    )?;
    if without_rowid == 1 {
        return Ok(());
    }
    let unsafe_index: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_index_list(?1) AS i
            WHERE i.name <> ('sqlite_autoindex_'||?1||'_1')
              AND i.name <> ?2)",
        params![table, allowed_index],
        |r| r.get(0),
    )?;
    let triggers: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if unsafe_index || triggers {
        return Err(unsafe_schema(&format!(
            "{table} has unreviewed indexes/triggers"
        )));
    }
    if table == "task_tags" {
        let valid_fk: bool = conn.query_row(
            "SELECT (SELECT count(*) FROM pragma_foreign_key_list('task_tags'))=1
             AND EXISTS(SELECT 1 FROM pragma_foreign_key_list('task_tags')
             WHERE \"table\"='tasks' AND \"from\"='task_id'
               AND \"to\"='id' AND on_delete='CASCADE')",
            [],
            |r| r.get(0),
        )?;
        if !valid_fk {
            return Err(unsafe_schema("task_tags foreign key is unexpected"));
        }
        let valid_tag_index: bool = conn.query_row(
            "SELECT NOT EXISTS(
                SELECT 1 FROM pragma_index_list('task_tags') AS i
                WHERE i.name='idx_task_tags_tag'
                  AND (i.\"unique\"<>0 OR i.partial<>0
                    OR (SELECT group_concat(name,',') FROM (
                        SELECT name FROM pragma_index_info(i.name)
                        ORDER BY seqno))<>'tag')
            )",
            [],
            |r| r.get(0),
        )?;
        if !valid_tag_index {
            return Err(unsafe_schema("tag reverse lookup index was redefined"));
        }
    }
    conn.execute_batch(ddl)?;
    conn.execute_batch(insert)?;
    let exact: bool = conn.query_row(compare, [], |r| r.get(0))?;
    if !exact {
        return Err(unsafe_schema(&format!(
            "{table} failed lossless copy comparison"
        )));
    }
    // This function is called only with the two constants above.
    conn.execute_batch(&format!("DROP TABLE \"{table}\""))?;
    conn.execute_batch(&format!(
        "ALTER TABLE \"__decapod_compact_{table}\" RENAME TO \"{table}\""
    ))?;
    if !secondary.is_empty() {
        conn.execute_batch(secondary)?;
    }
    Ok(())
}

/// Rewrite only cold, derived LCM summaries; do not change event history.
fn compact_summaries(conn: &Connection) -> Result<(), DecapodError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='summaries')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let mut cursor = String::new();
    loop {
        let mut stmt = conn.prepare(
            "SELECT summary_hash,original_hashes,summary_text FROM summaries
             WHERE summary_hash>?1 ORDER BY summary_hash LIMIT 16",
        )?;
        let mut rows = stmt.query([cursor.as_str()])?;
        let mut batch = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let params = row.parameters();
            let hashes = storage_codec::decode(&params[1])?;
            let summary = storage_codec::decode(&params[2])?;
            batch.push((id, params[1].clone(), params[2].clone(), hashes, summary));
        }
        if batch.is_empty() {
            break;
        }
        cursor = batch.last().expect("nonempty summary batch").0.clone();
        for (id, old_hashes, old_summary, hashes, summary) in batch {
            let new_hashes = storage_codec::encode(&hashes)?;
            let new_summary = storage_codec::encode(&summary)?;
            if matches!(old_hashes, Parameter::Text(_))
                && matches!(&new_hashes, StoredText::Zlib(_))
            {
                conn.execute(
                    "UPDATE summaries SET original_hashes=?1 WHERE summary_hash=?2",
                    params![new_hashes, id],
                )?;
            }
            if matches!(old_summary, Parameter::Text(_))
                && matches!(&new_summary, StoredText::Zlib(_))
            {
                conn.execute(
                    "UPDATE summaries SET summary_text=?1 WHERE summary_hash=?2",
                    params![new_summary, id],
                )?;
            }
            let physical: Vec<Parameter> = conn.query_row(
                "SELECT original_hashes,summary_text FROM summaries WHERE summary_hash=?1",
                [&id],
                |r| Ok(r.parameters()),
            )?;
            if storage_codec::decode(&physical[0])? != hashes
                || storage_codec::decode(&physical[1])? != summary
            {
                return Err(unsafe_schema("LCM text changed after compression"));
            }
        }
    }
    Ok(())
}
