//! Read-only projection performance budgets, indexed cursor replay, bounded
//! worktree enumeration, explicit degraded mode, and retention-stable receipts
//! (issue #1734).
//!
//! Boundary invariant: Decapod owns authoritative state, safe read contracts,
//! performance budgets, and cryptographic receipts. Zero UI dependencies or
//! display-specific state machines belong in this module. Every projection
//! surface is strictly read-only (`read_only = true`, `authoritative = false`)
//! and degrades explicitly rather than blocking or disrupting governed writers.

use crate::core::bounded_process::BoundedCommand;
use crate::core::db::{self, Connection, OptionalExtension, params};
use crate::core::error::DecapodError;
use crate::core::events::{self, StoredEvent};
use crate::core::schemas;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Versioned schema identifier for budgeted projection envelopes and receipts.
pub const PROJECTION_BUDGET_SCHEMA_VERSION: &str = "decapod.projection-budget.v1";

/// Versioned schema identifier for cryptographic event-retention watermarks.
pub const EVENT_RETENTION_ANCHOR_SCHEMA_VERSION: &str = "decapod.event-retention-anchor.v1";

/// Internal SQL keyset batch cap so Dactyl never materializes an unbounded
/// vector in memory before row/byte/wall-time budget checks run.
const SQL_BATCH_CAP: usize = 256;

/// Hard upper ceilings enforced on caller-supplied `ProjectionBudget` values.
pub const HARD_MAX_ROWS: usize = 4_096;
pub const HARD_MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const HARD_MAX_SCAN_ROWS: usize = 16_384;
pub const HARD_MAX_WORKTREES: usize = 256;
pub const HARD_MAX_FILES_PER_WORKTREE: usize = 2_048;
pub const HARD_MAX_WALL_TIME_MS: u64 = 10_000;
pub const HARD_MAX_READER_LOCK_TIMEOUT_MS: u64 = 2_000;

/// Measured-baseline default budget constants (calibrated from
/// `tests/projection_performance_budgets_1734.rs::projection_performance_baseline_benchmark_1734`):
/// - 10,000-event store (512B payloads): unbounded read materialized 7.04 MiB in
///   33 ms, while a 256-row / 512 KiB page materialized 179 KiB (146 KiB peak
///   SQL batch) at p50 = 3.93 ms / p95 = 4.16 ms.
/// - 4 concurrent WAL writers + 4 continuous read-only projection clients:
///   writer p50 shifted 62 us -> 77 us (p95 87 us -> 144 us) with 0 writer
///   errors and 0 reader contention deferrals at `reader_lock_timeout_ms = 150`.
/// - 25-worktree repository: metadata-only enumeration took 22 ms (6.3 KiB),
///   while per-worktree dirty summary took 462 ms (~18.5 ms/worktree); capping
///   default worktrees at 24 keeps full dirty sweeps inside the 500 ms wall budget.
pub const DEFAULT_MAX_ROWS: usize = 256;
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 512 * 1024;
pub const DEFAULT_MAX_SCAN_ROWS: usize = 2_048;
pub const DEFAULT_MAX_WORKTREES: usize = 24;
pub const DEFAULT_MAX_FILES_PER_WORKTREE: usize = 256;
pub const DEFAULT_MAX_WALL_TIME_MS: u64 = 500;
pub const DEFAULT_READER_LOCK_TIMEOUT_MS: u64 = 150;

/// Explicit resource and latency budget enforced on every read-only projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionBudget {
    /// Maximum logical event rows returned in a single page.
    pub max_rows: usize,
    /// Maximum canonical serialized payload bytes returned in a single page.
    pub max_response_bytes: usize,
    /// Maximum SQLite rows scanned before yielding (bounds sparse filter cost).
    pub max_scan_rows: usize,
    /// Maximum worktrees returned in a single repository inventory page.
    pub max_worktrees: usize,
    /// Maximum dirty/untracked file paths captured per worktree summary.
    pub max_files_per_worktree: usize,
    /// Maximum wall-clock execution budget in milliseconds before degrading.
    pub max_wall_time_ms: u64,
    /// Maximum read-only lock wait in milliseconds before deferring to writers.
    pub reader_lock_timeout_ms: u64,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self {
            max_rows: DEFAULT_MAX_ROWS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_scan_rows: DEFAULT_MAX_SCAN_ROWS,
            max_worktrees: DEFAULT_MAX_WORKTREES,
            max_files_per_worktree: DEFAULT_MAX_FILES_PER_WORKTREE,
            max_wall_time_ms: DEFAULT_MAX_WALL_TIME_MS,
            reader_lock_timeout_ms: DEFAULT_READER_LOCK_TIMEOUT_MS,
        }
    }
}

impl ProjectionBudget {
    /// Validate that every budget parameter is strictly positive and within the
    /// kernel's hard safety ceilings.
    pub fn validate(&self) -> Result<(), DecapodError> {
        if self.max_rows == 0 || self.max_rows > HARD_MAX_ROWS {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_rows must be in 1..={HARD_MAX_ROWS}, got {}",
                self.max_rows
            )));
        }
        if self.max_response_bytes == 0 || self.max_response_bytes > HARD_MAX_RESPONSE_BYTES {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_response_bytes must be in 1..={HARD_MAX_RESPONSE_BYTES}, got {}",
                self.max_response_bytes
            )));
        }
        if self.max_scan_rows == 0 || self.max_scan_rows > HARD_MAX_SCAN_ROWS {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_scan_rows must be in 1..={HARD_MAX_SCAN_ROWS}, got {}",
                self.max_scan_rows
            )));
        }
        if self.max_worktrees == 0 || self.max_worktrees > HARD_MAX_WORKTREES {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_worktrees must be in 1..={HARD_MAX_WORKTREES}, got {}",
                self.max_worktrees
            )));
        }
        if self.max_files_per_worktree == 0
            || self.max_files_per_worktree > HARD_MAX_FILES_PER_WORKTREE
        {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_files_per_worktree must be in 1..={HARD_MAX_FILES_PER_WORKTREE}, got {}",
                self.max_files_per_worktree
            )));
        }
        if self.max_wall_time_ms == 0 || self.max_wall_time_ms > HARD_MAX_WALL_TIME_MS {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: max_wall_time_ms must be in 1..={HARD_MAX_WALL_TIME_MS}, got {}",
                self.max_wall_time_ms
            )));
        }
        if self.reader_lock_timeout_ms == 0
            || self.reader_lock_timeout_ms > HARD_MAX_READER_LOCK_TIMEOUT_MS
        {
            return Err(DecapodError::ValidationError(format!(
                "PROJECTION_BUDGET_INVALID: reader_lock_timeout_ms must be in 1..={HARD_MAX_READER_LOCK_TIMEOUT_MS}, got {}",
                self.reader_lock_timeout_ms
            )));
        }
        Ok(())
    }
}

/// Honest completion status of a bounded projection call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionStatus {
    /// All matching records within the requested scope fit inside the budget.
    Complete,
    /// Cleanly paginated at the requested page boundary; resume via cursor.
    PartialHasMore,
    /// One or more resource, time, retention, or contention budgets tripped.
    /// `degraded_reasons` explains the exact cause; never claims completeness.
    Degraded,
}

/// Machine-checkable reasons why a projection entered explicit degraded mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum DegradedReason {
    ResponseByteBudgetExceeded {
        budget_bytes: usize,
        observed_bytes: usize,
    },
    ScanBudgetExceeded {
        budget_scan_rows: usize,
        scanned_rows: usize,
    },
    WorktreeBudgetExceeded {
        budget_worktrees: usize,
        observed_worktrees: usize,
    },
    FileEnumerationBudgetExceeded {
        worktree_path: String,
        budget_files: usize,
        observed_files: usize,
    },
    WallTimeBudgetExceeded {
        budget_ms: u64,
        elapsed_ms: u64,
    },
    CursorBehindRetentionFloor {
        requested_seq: u64,
        retention_floor_seq: u64,
    },
    ReaderContentionDeferred {
        timeout_ms: u64,
        detail: String,
    },
}

/// Measured runtime resource accounting emitted with every projection page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionMetrics {
    pub rows_scanned: usize,
    pub rows_emitted: usize,
    pub response_bytes: usize,
    pub peak_batch_bytes: usize,
    pub worktrees_scanned: usize,
    pub worktrees_emitted: usize,
    pub elapsed_ms: u64,
}

/// Relationship between a caller's cursor and the stream's retention floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorDisposition {
    /// Cursor resumed seamlessly within retained history.
    Continuous,
    /// Fresh read (`after_seq == 0`) starting at the post-retention floor.
    StartsAtRetentionFloor,
    /// Requested cursor (`0 < after_seq < retention_floor_seq`) fell behind
    /// already-retained history; anchor digest proves the pruned prefix.
    BehindRetentionFloor,
}

/// Cryptographic watermark recorded in `meta(namespace='event_retention')` when
/// explicit bounded event retention prunes cold events (`seq <= retention_floor_seq`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRetentionAnchor {
    pub schema_version: String,
    pub stream: String,
    pub retention_floor_seq: u64,
    pub last_pruned_event_id: String,
    pub cumulative_pruned_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_anchor_sha256: Option<String>,
    pub anchor_sha256: String,
    pub updated_at: String,
}

/// Report emitted by [`apply_bounded_event_retention`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRetentionReport {
    pub stream: String,
    pub total_before: u64,
    pub pruned_rows: u64,
    pub retained_rows: u64,
    pub retention_floor_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<EventRetentionAnchor>,
}

/// Read-only event projection request with optional subject and event-type filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventProjectionQuery {
    pub stream: String,
    #[serde(default)]
    pub after_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(default)]
    pub budget: ProjectionBudget,
}

impl EventProjectionQuery {
    pub fn for_stream(stream: impl Into<String>) -> Self {
        Self {
            stream: stream.into(),
            after_seq: 0,
            subject_kind: None,
            subject_id: None,
            event_type: None,
            budget: ProjectionBudget::default(),
        }
    }
}

/// Incremental keyset cursor returned with every event projection page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventProjectionCursor {
    pub stream: String,
    pub after_seq: u64,
    pub next_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_id: Option<String>,
    pub retention_floor_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_anchor_sha256: Option<String>,
    pub disposition: CursorDisposition,
    pub has_more: bool,
}

/// Strictly read-only, budget-bounded event projection response and receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventProjectionPage {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub status: ProjectionStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded_reasons: Vec<DegradedReason>,
    pub cursor: EventProjectionCursor,
    pub metrics: ProjectionMetrics,
    pub budget: ProjectionBudget,
    /// Canonical SHA-256 over emitted reconstructed logical events (invariant
    /// across lossless compaction, component GC, VACUUM, and older retention).
    pub content_sha256: String,
    /// Stable window receipt binding `(schema_version, stream, first_seq, last_seq, count, content_sha256)`.
    pub window_receipt_sha256: String,
    /// Retention-aware continuity receipt binding `(retention_floor_seq, retention_anchor_sha256, window_receipt_sha256)`.
    pub continuity_receipt_sha256: String,
    pub items: Vec<StoredEvent>,
}

/// Read-only repository worktree enumeration request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeProjectionQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_worktree_path: Option<String>,
    #[serde(default)]
    pub include_dirty_summary: bool,
    #[serde(default)]
    pub budget: ProjectionBudget,
}

/// Bounded read-only view of one Git worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeProjectionEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_oid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub prunable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_file_count: Option<usize>,
    pub dirty_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_dirty_paths: Vec<String>,
}

/// Strictly read-only, budget-bounded worktree enumeration response and receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeProjectionPage {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub status: ProjectionStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded_reasons: Vec<DegradedReason>,
    pub total_discovered: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub metrics: ProjectionMetrics,
    pub budget: ProjectionBudget,
    pub content_sha256: String,
    pub receipt_sha256: String,
    pub items: Vec<WorktreeProjectionEntry>,
}

/// Canonicalize a JSON value by recursively sorting all object keys so logical
/// hashes are completely invariant to physical JSON key ordering and `_ce_v1`
/// reconstruction.
pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    fn sort_value(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut sorted: BTreeMap<&String, Value> = BTreeMap::new();
                for (k, val) in map {
                    sorted.insert(k, sort_value(val));
                }
                let mut out = Map::with_capacity(sorted.len());
                for (k, val) in sorted {
                    out.insert(k.clone(), val);
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_vec(&sort_value(value)).unwrap_or_default()
}

fn update_event_hash(hasher: &mut Sha256, event: &StoredEvent) {
    hasher.update(event.stream.as_bytes());
    hasher.update([0]);
    hasher.update(event.seq.to_le_bytes());
    hasher.update(event.event_id.as_bytes());
    hasher.update([0]);
    hasher.update(event.ts.as_bytes());
    hasher.update([0]);
    hasher.update(event.event_type.as_bytes());
    hasher.update([0]);
    hasher.update(event.actor.as_bytes());
    hasher.update([0]);
    hasher.update(canonical_json_bytes(&event.payload));
    hasher.update([0xff]);
}

fn compute_events_receipts(
    stream: &str,
    items: &[StoredEvent],
    retention_floor_seq: u64,
    retention_anchor_sha256: Option<&str>,
) -> (String, String, String) {
    let mut content_hasher = Sha256::new();
    for event in items {
        update_event_hash(&mut content_hasher, event);
    }
    let content_sha256 = format!("sha256:{:x}", content_hasher.finalize());

    let first_seq = items.first().map(|e| e.seq).unwrap_or(0);
    let last_seq = items.last().map(|e| e.seq).unwrap_or(0);
    let mut window_hasher = Sha256::new();
    window_hasher.update(PROJECTION_BUDGET_SCHEMA_VERSION.as_bytes());
    window_hasher.update([0]);
    window_hasher.update(stream.as_bytes());
    window_hasher.update([0]);
    window_hasher.update(first_seq.to_le_bytes());
    window_hasher.update(last_seq.to_le_bytes());
    window_hasher.update((items.len() as u64).to_le_bytes());
    window_hasher.update(content_sha256.as_bytes());
    let window_receipt_sha256 = format!("sha256:{:x}", window_hasher.finalize());

    let mut continuity_hasher = Sha256::new();
    continuity_hasher.update(retention_floor_seq.to_le_bytes());
    continuity_hasher.update(retention_anchor_sha256.unwrap_or("none").as_bytes());
    continuity_hasher.update([0]);
    continuity_hasher.update(window_receipt_sha256.as_bytes());
    let continuity_receipt_sha256 = format!("sha256:{:x}", continuity_hasher.finalize());

    (
        content_sha256,
        window_receipt_sha256,
        continuity_receipt_sha256,
    )
}

/// Read the current cryptographic retention anchor for a stream, if retention
/// has ever pruned older events from that stream.
pub fn read_retention_anchor(
    conn: &Connection,
    stream: &str,
) -> Result<Option<EventRetentionAnchor>, DecapodError> {
    if !events::is_known_stream(stream) {
        return Err(DecapodError::ValidationError(format!(
            "unknown event stream: {stream}"
        )));
    }
    if !conn.has_table("meta")? {
        return Ok(None);
    }
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE namespace = ?1 AND key = ?2",
            params![schemas::EVENT_RETENTION_META_NAMESPACE, stream],
            |row| row.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let anchor: EventRetentionAnchor = serde_json::from_str(&raw).map_err(|err| {
        DecapodError::ValidationError(format!(
            "EVENT_RETENTION_ANCHOR_CORRUPT: invalid retention anchor for stream '{stream}': {err}"
        ))
    })?;
    Ok(Some(anchor))
}

/// Fast internal lookup used by `events::append_on_conn` only when a stream has
/// zero live rows, guaranteeing strictly monotonic sequences across full prune.
pub(crate) fn retention_floor_seq_on_conn(
    conn: &Connection,
    stream: &str,
) -> Result<u64, DecapodError> {
    Ok(read_retention_anchor(conn, stream)?
        .map(|a| a.retention_floor_seq)
        .unwrap_or(0))
}

/// Explicitly apply bounded cold-window event retention on a stream, keeping
/// the newest `keep_latest` events and replacing pruned older events with a
/// chained cryptographic `EventRetentionAnchor` in `meta`.
///
/// Surviving events retain their exact original `seq` and `event_id` so active
/// projection cursors and retained-window receipts remain bit-for-bit stable.
/// Never executed automatically on hot read paths.
pub fn apply_bounded_event_retention(
    conn: &Connection,
    stream: &str,
    keep_latest: u64,
) -> Result<EventRetentionReport, DecapodError> {
    if !events::is_known_stream(stream) {
        return Err(DecapodError::ValidationError(format!(
            "unknown event stream: {stream}"
        )));
    }
    if !conn.has_table(events::EVENTS_TABLE)? {
        return Ok(EventRetentionReport {
            stream: stream.to_string(),
            total_before: 0,
            pruned_rows: 0,
            retained_rows: 0,
            retention_floor_seq: 0,
            anchor: None,
        });
    }

    let owns_tx = conn.is_autocommit();
    if owns_tx {
        conn.execute_batch("BEGIN IMMEDIATE")?;
    }

    let outcome = (|| -> Result<EventRetentionReport, DecapodError> {
        conn.execute(schemas::TODO_DB_SCHEMA_META, [])?;
        let prev_anchor = read_retention_anchor(conn, stream)?;
        let prev_floor = prev_anchor
            .as_ref()
            .map(|a| a.retention_floor_seq)
            .unwrap_or(0);
        let total_before: u64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE stream = ?1",
            [stream],
            |row| row.get(0),
        )?;

        if total_before <= keep_latest {
            return Ok(EventRetentionReport {
                stream: stream.to_string(),
                total_before,
                pruned_rows: 0,
                retained_rows: total_before,
                retention_floor_seq: prev_floor,
                anchor: prev_anchor,
            });
        }

        let to_prune = total_before - keep_latest;
        let mut hasher = Sha256::new();
        hasher.update(EVENT_RETENTION_ANCHOR_SCHEMA_VERSION.as_bytes());
        hasher.update([0]);
        hasher.update(stream.as_bytes());
        hasher.update([0]);
        if let Some(prev) = &prev_anchor {
            hasher.update(prev.anchor_sha256.as_bytes());
        } else {
            hasher.update(b"genesis");
        }
        hasher.update([0]);

        let mut pruned_so_far = 0u64;
        let mut last_seq = prev_floor;
        let mut last_event_id = String::new();

        while pruned_so_far < to_prune {
            let remaining = (to_prune - pruned_so_far).min(SQL_BATCH_CAP as u64);
            let mut stmt = conn.prepare(
                "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
                 FROM events
                 WHERE stream = ?1 AND seq > ?2
                 ORDER BY seq ASC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![stream, last_seq, remaining], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })?;
            let batch = rows.collect::<Result<Vec<_>, _>>()?;
            if batch.is_empty() {
                break;
            }
            for (event_id, ts, seq, event_type, raw_payload, actor, subject_kind, subject_id) in
                batch
            {
                let payload = events::reconstruct_stored_payload(
                    stream,
                    &event_id,
                    &ts,
                    &event_type,
                    &actor,
                    subject_kind.as_deref(),
                    subject_id.as_deref(),
                    &raw_payload,
                )?;
                let stored = StoredEvent {
                    stream: stream.to_string(),
                    event_id: event_id.clone(),
                    ts,
                    seq,
                    event_type,
                    payload,
                    actor,
                };
                update_event_hash(&mut hasher, &stored);
                last_seq = seq;
                last_event_id = event_id;
                pruned_so_far += 1;
            }
        }

        let deleted = conn.execute(
            "DELETE FROM events WHERE stream = ?1 AND seq <= ?2",
            params![stream, last_seq],
        )?;
        let cumulative_pruned_count = prev_anchor
            .as_ref()
            .map(|a| a.cumulative_pruned_count)
            .unwrap_or(0)
            .saturating_add(deleted as u64);
        let anchor = EventRetentionAnchor {
            schema_version: EVENT_RETENTION_ANCHOR_SCHEMA_VERSION.to_string(),
            stream: stream.to_string(),
            retention_floor_seq: last_seq,
            last_pruned_event_id: last_event_id,
            cumulative_pruned_count,
            prev_anchor_sha256: prev_anchor.map(|a| a.anchor_sha256),
            anchor_sha256: format!("sha256:{:x}", hasher.finalize()),
            updated_at: crate::core::time::now_rfc3339(),
        };
        let encoded = serde_json::to_string(&anchor).map_err(|err| {
            DecapodError::ValidationError(format!(
                "failed to serialize event retention anchor: {err}"
            ))
        })?;
        conn.execute(
            "INSERT INTO meta(namespace, key, value) VALUES(?1, ?2, ?3)
             ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value",
            params![schemas::EVENT_RETENTION_META_NAMESPACE, stream, encoded],
        )?;

        let retained_rows: u64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE stream = ?1",
            [stream],
            |row| row.get(0),
        )?;

        Ok(EventRetentionReport {
            stream: stream.to_string(),
            total_before,
            pruned_rows: deleted as u64,
            retained_rows,
            retention_floor_seq: last_seq,
            anchor: Some(anchor),
        })
    })();

    if owns_tx {
        match &outcome {
            Ok(_) => conn.execute_batch("COMMIT")?,
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK");
            }
        }
    }
    outcome
}

fn is_contention_error(err: &DecapodError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("storage_lock_timeout")
        || text.contains("locked")
        || text.contains("busy")
        || text.contains("contention")
}

/// Strictly read-only, bounded event projection over `.decapod/data/decapod.db`.
///
/// Opens the datastore via [`db::db_connect_readonly_bounded`] with
/// `AccessMode::ReadOnly` and `StorageLockMode::Shared`. If an exclusive
/// maintenance operation or heavy write burst prevents opening within
/// `budget.reader_lock_timeout_ms`, returns a typed degraded-mode page
/// (`DegradedReason::ReaderContentionDeferred`) instead of stalling or
/// disrupting writers.
pub fn project_events_read_only(
    data_root: &Path,
    query: &EventProjectionQuery,
) -> Result<EventProjectionPage, DecapodError> {
    query.budget.validate()?;
    if !events::is_known_stream(&query.stream) {
        return Err(DecapodError::ValidationError(format!(
            "unknown event stream: {}",
            query.stream
        )));
    }
    let started = Instant::now();
    let db_path = events::canonical_db_path(data_root);
    if !db_path.exists() {
        let (content_sha256, window_receipt_sha256, continuity_receipt_sha256) =
            compute_events_receipts(&query.stream, &[], 0, None);
        return Ok(EventProjectionPage {
            schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
            read_only: true,
            authoritative: false,
            status: ProjectionStatus::Complete,
            degraded_reasons: Vec::new(),
            cursor: EventProjectionCursor {
                stream: query.stream.clone(),
                after_seq: query.after_seq,
                next_seq: query.after_seq,
                last_event_id: None,
                retention_floor_seq: 0,
                retention_anchor_sha256: None,
                disposition: CursorDisposition::Continuous,
                has_more: false,
            },
            metrics: ProjectionMetrics {
                elapsed_ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
            budget: query.budget.clone(),
            content_sha256,
            window_receipt_sha256,
            continuity_receipt_sha256,
            items: Vec::new(),
        });
    }

    let timeout = Duration::from_millis(query.budget.reader_lock_timeout_ms);
    let conn = match db::db_connect_readonly_bounded(&db_path.to_string_lossy(), timeout) {
        Ok(conn) => conn,
        Err(err) if is_contention_error(&err) => {
            let (content_sha256, window_receipt_sha256, continuity_receipt_sha256) =
                compute_events_receipts(&query.stream, &[], 0, None);
            return Ok(EventProjectionPage {
                schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
                read_only: true,
                authoritative: false,
                status: ProjectionStatus::Degraded,
                degraded_reasons: vec![DegradedReason::ReaderContentionDeferred {
                    timeout_ms: query.budget.reader_lock_timeout_ms,
                    detail: err.to_string(),
                }],
                cursor: EventProjectionCursor {
                    stream: query.stream.clone(),
                    after_seq: query.after_seq,
                    next_seq: query.after_seq,
                    last_event_id: None,
                    retention_floor_seq: 0,
                    retention_anchor_sha256: None,
                    disposition: CursorDisposition::Continuous,
                    has_more: true,
                },
                metrics: ProjectionMetrics {
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                },
                budget: query.budget.clone(),
                content_sha256,
                window_receipt_sha256,
                continuity_receipt_sha256,
                items: Vec::new(),
            });
        }
        Err(err) => return Err(err),
    };

    let deadline = started + Duration::from_millis(query.budget.max_wall_time_ms);
    match project_events_on_conn_with_deadline(&conn, query, started, deadline) {
        Ok(page) => Ok(page),
        Err(err) if is_contention_error(&err) => {
            let (content_sha256, window_receipt_sha256, continuity_receipt_sha256) =
                compute_events_receipts(&query.stream, &[], 0, None);
            Ok(EventProjectionPage {
                schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
                read_only: true,
                authoritative: false,
                status: ProjectionStatus::Degraded,
                degraded_reasons: vec![DegradedReason::ReaderContentionDeferred {
                    timeout_ms: query.budget.reader_lock_timeout_ms,
                    detail: err.to_string(),
                }],
                cursor: EventProjectionCursor {
                    stream: query.stream.clone(),
                    after_seq: query.after_seq,
                    next_seq: query.after_seq,
                    last_event_id: None,
                    retention_floor_seq: 0,
                    retention_anchor_sha256: None,
                    disposition: CursorDisposition::Continuous,
                    has_more: true,
                },
                metrics: ProjectionMetrics {
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                },
                budget: query.budget.clone(),
                content_sha256,
                window_receipt_sha256,
                continuity_receipt_sha256,
                items: Vec::new(),
            })
        }
        Err(err) => Err(err),
    }
}

/// Strictly read-only, bounded event projection over an existing connection.
pub fn project_events_on_conn(
    conn: &Connection,
    query: &EventProjectionQuery,
) -> Result<EventProjectionPage, DecapodError> {
    query.budget.validate()?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(query.budget.max_wall_time_ms);
    project_events_on_conn_with_deadline(conn, query, started, deadline)
}

/// Deterministic deadline-injectable entry point for budget enforcement tests.
pub fn project_events_on_conn_with_deadline(
    conn: &Connection,
    query: &EventProjectionQuery,
    started: Instant,
    deadline: Instant,
) -> Result<EventProjectionPage, DecapodError> {
    query.budget.validate()?;
    if !events::is_known_stream(&query.stream) {
        return Err(DecapodError::ValidationError(format!(
            "unknown event stream: {}",
            query.stream
        )));
    }
    if query.subject_kind.is_some() != query.subject_id.is_some() {
        return Err(DecapodError::ValidationError(
            "PROJECTION_QUERY_INVALID: subject_kind and subject_id must be specified together"
                .into(),
        ));
    }

    let anchor = read_retention_anchor(conn, &query.stream)?;
    let retention_floor_seq = anchor.as_ref().map(|a| a.retention_floor_seq).unwrap_or(0);
    let retention_anchor_sha256 = anchor.as_ref().map(|a| a.anchor_sha256.clone());

    let mut degraded_reasons = Vec::new();
    let disposition = if retention_floor_seq > 0 && query.after_seq == 0 {
        CursorDisposition::StartsAtRetentionFloor
    } else if retention_floor_seq > 0 && query.after_seq < retention_floor_seq {
        degraded_reasons.push(DegradedReason::CursorBehindRetentionFloor {
            requested_seq: query.after_seq,
            retention_floor_seq,
        });
        CursorDisposition::BehindRetentionFloor
    } else {
        CursorDisposition::Continuous
    };

    if !conn.has_table(events::EVENTS_TABLE)? {
        let (content_sha256, window_receipt_sha256, continuity_receipt_sha256) =
            compute_events_receipts(
                &query.stream,
                &[],
                retention_floor_seq,
                retention_anchor_sha256.as_deref(),
            );
        let status = if degraded_reasons.is_empty() {
            ProjectionStatus::Complete
        } else {
            ProjectionStatus::Degraded
        };
        return Ok(EventProjectionPage {
            schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
            read_only: true,
            authoritative: false,
            status,
            degraded_reasons,
            cursor: EventProjectionCursor {
                stream: query.stream.clone(),
                after_seq: query.after_seq,
                next_seq: query.after_seq.max(retention_floor_seq),
                last_event_id: None,
                retention_floor_seq,
                retention_anchor_sha256,
                disposition,
                has_more: false,
            },
            metrics: ProjectionMetrics {
                elapsed_ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
            budget: query.budget.clone(),
            content_sha256,
            window_receipt_sha256,
            continuity_receipt_sha256,
            items: Vec::new(),
        });
    }

    let mut items = Vec::new();
    let mut metrics = ProjectionMetrics::default();
    let mut scan_cursor_seq = query.after_seq.max(retention_floor_seq);
    let mut emitted_cursor_seq = query.after_seq.max(retention_floor_seq);
    let mut last_event_id: Option<String> = None;
    let mut has_more = false;

    loop {
        if Instant::now() >= deadline {
            degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                budget_ms: query.budget.max_wall_time_ms,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            has_more = true;
            break;
        }
        if metrics.rows_scanned >= query.budget.max_scan_rows {
            degraded_reasons.push(DegradedReason::ScanBudgetExceeded {
                budget_scan_rows: query.budget.max_scan_rows,
                scanned_rows: metrics.rows_scanned,
            });
            has_more = true;
            break;
        }

        let remaining_scan = query.budget.max_scan_rows - metrics.rows_scanned;
        let remaining_rows = query.budget.max_rows.saturating_sub(items.len());
        let batch_limit = remaining_rows
            .saturating_add(1)
            .min(SQL_BATCH_CAP)
            .min(remaining_scan.saturating_add(1))
            .max(1) as i64;

        type RawEventRow = (
            String,
            String,
            u64,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        );

        let batch: Vec<RawEventRow> = if let (Some(sk), Some(sid)) =
            (query.subject_kind.as_deref(), query.subject_id.as_deref())
        {
            let mut stmt = conn.prepare(
                "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
                 FROM events
                 WHERE stream = ?1 AND subject_kind = ?2 AND subject_id = ?3
                   AND subject_kind IS NOT NULL AND subject_id IS NOT NULL
                   AND seq > ?4
                 ORDER BY seq ASC
                 LIMIT ?5",
            )?;
            let rows = stmt.query_map(
                params![&query.stream, sk, sid, scan_cursor_seq, batch_limit],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT event_id, ts, seq, event_type, payload, actor, subject_kind, subject_id
                 FROM events
                 WHERE stream = ?1 AND seq > ?2
                 ORDER BY seq ASC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(
                params![&query.stream, scan_cursor_seq, batch_limit],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        if batch.is_empty() {
            break;
        }

        let batch_raw_bytes: usize = batch
            .iter()
            .map(|r| r.4.len() + r.0.len() + r.3.len())
            .sum();
        metrics.peak_batch_bytes = metrics.peak_batch_bytes.max(batch_raw_bytes);

        let batch_len = batch.len();
        let mut stop_loop = false;

        for (event_id, ts, seq, event_type, raw_payload, actor, subject_kind, subject_id) in batch {
            if Instant::now() >= deadline {
                degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                    budget_ms: query.budget.max_wall_time_ms,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
                has_more = true;
                stop_loop = true;
                break;
            }
            if metrics.rows_scanned >= query.budget.max_scan_rows {
                degraded_reasons.push(DegradedReason::ScanBudgetExceeded {
                    budget_scan_rows: query.budget.max_scan_rows,
                    scanned_rows: metrics.rows_scanned,
                });
                has_more = true;
                stop_loop = true;
                break;
            }
            if items.len() >= query.budget.max_rows {
                // This row proves at least one more matching row exists beyond max_rows.
                if query
                    .event_type
                    .as_deref()
                    .is_none_or(|wanted| wanted == event_type)
                {
                    has_more = true;
                    stop_loop = true;
                    break;
                }
                metrics.rows_scanned += 1;
                scan_cursor_seq = seq;
                continue;
            }

            metrics.rows_scanned += 1;
            scan_cursor_seq = seq;

            if query
                .event_type
                .as_deref()
                .is_some_and(|wanted| wanted != event_type)
            {
                continue;
            }

            let payload = events::reconstruct_stored_payload(
                &query.stream,
                &event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &raw_payload,
            )?;
            let stored = StoredEvent {
                stream: query.stream.clone(),
                event_id: event_id.clone(),
                ts,
                seq,
                event_type,
                payload,
                actor,
            };

            let item_bytes = serde_json::to_vec(&stored)
                .map_err(|err| {
                    DecapodError::ValidationError(format!(
                        "failed to size projected event {event_id}: {err}"
                    ))
                })?
                .len();
            let projected_total = metrics.response_bytes.saturating_add(item_bytes);
            if projected_total > query.budget.max_response_bytes {
                degraded_reasons.push(DegradedReason::ResponseByteBudgetExceeded {
                    budget_bytes: query.budget.max_response_bytes,
                    observed_bytes: projected_total,
                });
                has_more = true;
                stop_loop = true;
                break;
            }

            metrics.response_bytes = projected_total;
            emitted_cursor_seq = seq;
            last_event_id = Some(event_id);
            items.push(stored);
            metrics.rows_emitted = items.len();
        }

        if stop_loop || (batch_len as i64) < batch_limit {
            break;
        }
    }

    metrics.elapsed_ms = started.elapsed().as_millis() as u64;

    let next_seq = if items.is_empty()
        && degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::ResponseByteBudgetExceeded { .. }))
    {
        // Keep cursor before the oversized first record so the caller can retry
        // with a larger byte budget without silently skipping it.
        query.after_seq.max(retention_floor_seq)
    } else if items.is_empty() && query.event_type.is_some() {
        // Advance past non-matching scanned rows when scan budget tripped.
        scan_cursor_seq
    } else {
        emitted_cursor_seq
    };

    let status = if !degraded_reasons.is_empty() {
        ProjectionStatus::Degraded
    } else if has_more {
        ProjectionStatus::PartialHasMore
    } else {
        ProjectionStatus::Complete
    };

    let (content_sha256, window_receipt_sha256, continuity_receipt_sha256) =
        compute_events_receipts(
            &query.stream,
            &items,
            retention_floor_seq,
            retention_anchor_sha256.as_deref(),
        );

    Ok(EventProjectionPage {
        schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        status,
        degraded_reasons,
        cursor: EventProjectionCursor {
            stream: query.stream.clone(),
            after_seq: query.after_seq,
            next_seq,
            last_event_id,
            retention_floor_seq,
            retention_anchor_sha256,
            disposition,
            has_more,
        },
        metrics,
        budget: query.budget.clone(),
        content_sha256,
        window_receipt_sha256,
        continuity_receipt_sha256,
        items,
    })
}

#[derive(Debug, Default)]
struct ParsedWorktreeStanza {
    path: Option<String>,
    head_oid: Option<String>,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    prunable: bool,
}

fn parse_worktree_porcelain(stdout: &str) -> Vec<WorktreeProjectionEntry> {
    let mut entries = Vec::new();
    let mut current = ParsedWorktreeStanza::default();

    let flush = |stanza: &mut ParsedWorktreeStanza, out: &mut Vec<WorktreeProjectionEntry>| {
        if let Some(path) = stanza.path.take() {
            out.push(WorktreeProjectionEntry {
                path,
                head_oid: stanza.head_oid.take(),
                branch: stanza.branch.take(),
                bare: stanza.bare,
                detached: stanza.detached,
                prunable: stanza.prunable,
                dirty_file_count: None,
                dirty_truncated: false,
                sample_dirty_paths: Vec::new(),
            });
        }
        *stanza = ParsedWorktreeStanza::default();
    };

    for raw_line in stdout.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            flush(&mut current, &mut entries);
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            flush(&mut current, &mut entries);
            current.path = Some(path.to_string());
        } else if let Some(oid) = line.strip_prefix("HEAD ") {
            current.head_oid = Some(oid.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line == "bare" {
            current.bare = true;
        } else if line == "detached" {
            current.detached = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            current.prunable = true;
        }
    }
    flush(&mut current, &mut entries);
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

fn bounded_dirty_summary(
    worktree_path: &Path,
    max_files: usize,
    timeout: Duration,
) -> Result<(usize, bool, Vec<String>), std::io::Error> {
    let output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
        .bounded_output(timeout.max(Duration::from_millis(25)))?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "git status failed for {}",
            worktree_path.display()
        )));
    }
    let mut entries = output
        .stdout
        .split(|b| *b == 0)
        .filter(|slice| !slice.is_empty());
    let mut count = 0usize;
    let mut truncated = false;
    let mut sample = Vec::new();
    while let Some(record) = entries.next() {
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }
        let status_bytes = &record[..2];
        let path = String::from_utf8_lossy(&record[3..]).into_owned();
        if status_bytes.contains(&b'R') || status_bytes.contains(&b'C') {
            let _ = entries.next();
        }
        count += 1;
        if sample.len() < max_files {
            sample.push(path);
        } else {
            truncated = true;
        }
    }
    sample.sort();
    Ok((count, truncated, sample))
}

fn compute_worktree_receipts(
    items: &[WorktreeProjectionEntry],
    total_discovered: usize,
) -> (String, String) {
    let mut content_hasher = Sha256::new();
    for item in items {
        let encoded = serde_json::to_vec(item).unwrap_or_default();
        content_hasher.update(&encoded);
        content_hasher.update([0xff]);
    }
    let content_sha256 = format!("sha256:{:x}", content_hasher.finalize());

    let mut receipt_hasher = Sha256::new();
    receipt_hasher.update(PROJECTION_BUDGET_SCHEMA_VERSION.as_bytes());
    receipt_hasher.update([0]);
    receipt_hasher.update((total_discovered as u64).to_le_bytes());
    receipt_hasher.update((items.len() as u64).to_le_bytes());
    receipt_hasher.update(content_sha256.as_bytes());
    let receipt_sha256 = format!("sha256:{:x}", receipt_hasher.finalize());

    (content_sha256, receipt_sha256)
}

/// Strictly read-only, lock-free (`GIT_OPTIONAL_LOCKS=0`), budget-bounded
/// repository worktree enumeration.
pub fn project_worktrees_read_only(
    repo_root: &Path,
    query: &WorktreeProjectionQuery,
) -> Result<WorktreeProjectionPage, DecapodError> {
    query.budget.validate()?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(query.budget.max_wall_time_ms);
    project_worktrees_read_only_with_deadline(repo_root, query, started, deadline)
}

/// Deterministic deadline-injectable entry point for worktree budget tests.
pub fn project_worktrees_read_only_with_deadline(
    repo_root: &Path,
    query: &WorktreeProjectionQuery,
    started: Instant,
    deadline: Instant,
) -> Result<WorktreeProjectionPage, DecapodError> {
    query.budget.validate()?;
    let mut degraded_reasons = Vec::new();
    let mut metrics = ProjectionMetrics::default();

    let now = Instant::now();
    if now >= deadline {
        degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
            budget_ms: query.budget.max_wall_time_ms,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
        let (content_sha256, receipt_sha256) = compute_worktree_receipts(&[], 0);
        metrics.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(WorktreeProjectionPage {
            schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
            read_only: true,
            authoritative: false,
            status: ProjectionStatus::Degraded,
            degraded_reasons,
            total_discovered: 0,
            next_cursor: query.after_worktree_path.clone(),
            has_more: true,
            metrics,
            budget: query.budget.clone(),
            content_sha256,
            receipt_sha256,
            items: Vec::new(),
        });
    }

    let remaining = deadline.saturating_duration_since(now);
    let output = match Command::new("git")
        .current_dir(repo_root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["worktree", "list", "--porcelain"])
        .bounded_output(remaining.max(Duration::from_millis(25)))
    {
        Ok(out) => out,
        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
            degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                budget_ms: query.budget.max_wall_time_ms,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            let (content_sha256, receipt_sha256) = compute_worktree_receipts(&[], 0);
            metrics.elapsed_ms = started.elapsed().as_millis() as u64;
            return Ok(WorktreeProjectionPage {
                schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
                read_only: true,
                authoritative: false,
                status: ProjectionStatus::Degraded,
                degraded_reasons,
                total_discovered: 0,
                next_cursor: query.after_worktree_path.clone(),
                has_more: true,
                metrics,
                budget: query.budget.clone(),
                content_sha256,
                receipt_sha256,
                items: Vec::new(),
            });
        }
        Err(err) => return Err(DecapodError::IoError(err)),
    };

    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "WORKTREE_PROJECTION_FAILED: git worktree list failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let all_entries = parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout));
    let total_discovered = all_entries.len();
    let eligible: Vec<WorktreeProjectionEntry> = all_entries
        .into_iter()
        .filter(|entry| {
            query
                .after_worktree_path
                .as_deref()
                .is_none_or(|cursor| entry.path.as_str() > cursor)
        })
        .collect();

    let mut items = Vec::new();
    let mut has_more = false;
    let mut next_cursor: Option<String> = query.after_worktree_path.clone();

    for mut entry in eligible {
        if Instant::now() >= deadline {
            degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                budget_ms: query.budget.max_wall_time_ms,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            has_more = true;
            break;
        }
        if items.len() >= query.budget.max_worktrees {
            degraded_reasons.push(DegradedReason::WorktreeBudgetExceeded {
                budget_worktrees: query.budget.max_worktrees,
                observed_worktrees: total_discovered,
            });
            has_more = true;
            break;
        }

        metrics.worktrees_scanned += 1;

        if query.include_dirty_summary && !entry.bare && !entry.prunable {
            let wt_path = Path::new(&entry.path);
            if wt_path.is_dir() {
                let rem = deadline.saturating_duration_since(Instant::now());
                if rem.is_zero() {
                    degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                        budget_ms: query.budget.max_wall_time_ms,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    });
                    has_more = true;
                    break;
                }
                match bounded_dirty_summary(wt_path, query.budget.max_files_per_worktree, rem) {
                    Ok((count, truncated, sample)) => {
                        entry.dirty_file_count = Some(count);
                        entry.dirty_truncated = truncated;
                        entry.sample_dirty_paths = sample;
                        if truncated {
                            degraded_reasons.push(DegradedReason::FileEnumerationBudgetExceeded {
                                worktree_path: entry.path.clone(),
                                budget_files: query.budget.max_files_per_worktree,
                                observed_files: count,
                            });
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                        degraded_reasons.push(DegradedReason::WallTimeBudgetExceeded {
                            budget_ms: query.budget.max_wall_time_ms,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        });
                        has_more = true;
                        break;
                    }
                    Err(_) => {
                        // Worktree may have been removed concurrently by another agent;
                        // mark prunable honestly rather than failing the whole read.
                        entry.prunable = true;
                    }
                }
            }
        }

        let item_bytes = serde_json::to_vec(&entry)
            .map_err(|err| {
                DecapodError::ValidationError(format!(
                    "failed to size worktree projection entry: {err}"
                ))
            })?
            .len();
        let projected_bytes = metrics.response_bytes.saturating_add(item_bytes);
        if projected_bytes > query.budget.max_response_bytes {
            degraded_reasons.push(DegradedReason::ResponseByteBudgetExceeded {
                budget_bytes: query.budget.max_response_bytes,
                observed_bytes: projected_bytes,
            });
            has_more = true;
            break;
        }
        metrics.response_bytes = projected_bytes;
        next_cursor = Some(entry.path.clone());
        items.push(entry);
        metrics.worktrees_emitted = items.len();
    }

    metrics.elapsed_ms = started.elapsed().as_millis() as u64;
    let status = if !degraded_reasons.is_empty() {
        ProjectionStatus::Degraded
    } else if has_more {
        ProjectionStatus::PartialHasMore
    } else {
        ProjectionStatus::Complete
    };
    let (content_sha256, receipt_sha256) = compute_worktree_receipts(&items, total_discovered);

    Ok(WorktreeProjectionPage {
        schema_version: PROJECTION_BUDGET_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        status,
        degraded_reasons,
        total_discovered,
        next_cursor: if has_more { next_cursor } else { None },
        has_more,
        metrics,
        budget: query.budget.clone(),
        content_sha256,
        receipt_sha256,
        items,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/core/projection_budget_tests.rs"]
mod tests;
