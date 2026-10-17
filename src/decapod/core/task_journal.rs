//! Versioned task projection checkpoints in the canonical TODO event stream.
//!
//! Legacy domain events are evidence, not a complete replay source. The first
//! checkpoint explicitly observes existing state; only subsequent complete
//! checkpoints form a reconstructible chain. Proof strings are preserved exactly.

use crate::core::{
    db::{Connection, params},
    error::DecapodError,
    events,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[path = "task_transfer.rs"]
pub(super) mod transfer;

pub(super) const EVENT_TYPE: &str = "task.journal.checkpoint";
const SCHEMA_VERSION: u32 = 1;
const MAX_TASKS: usize = 10_000;
const MAX_RECORDS: usize = 100_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;

fn invalid(message: impl AsRef<str>) -> DecapodError {
    DecapodError::ValidationError(format!("TASK_JOURNAL_CONFLICT: {}", message.as_ref()))
}
fn digest<T: Serialize>(value: &T) -> Result<String, DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn require_transaction(conn: &Connection) -> Result<(), DecapodError> {
    if conn.is_autocommit() {
        return Err(invalid("mutation requires the caller's transaction"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TaskRow {
    id: String,
    hash: String,
    title: String,
    description: Option<String>,
    tags: Option<String>,
    owner: Option<String>,
    due: Option<String>,
    r#ref: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    closed_at: Option<String>,
    dir_path: String,
    scope: String,
    parent_task_id: Option<String>,
    priority: Option<String>,
    depends_on: Option<String>,
    blocks: Option<String>,
    category: Option<String>,
    component: Option<String>,
    assigned_to: Option<String>,
    assigned_at: Option<String>,
    one_shot: Option<i64>,
    lease_expires_at: Option<String>,
    lease_generation: Option<i64>,
    lease_lifecycle: Option<String>,
    intent_anchor: Option<String>,
    revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OwnerRow {
    id: String,
    agent_id: String,
    claimed_at: String,
    claim_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DependencyRow {
    id: String,
    depends_on_task_id: String,
    created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VerificationRow {
    proof_plan: String,
    verification_artifacts: Option<String>,
    last_verified_at: Option<String>,
    last_verified_status: Option<String>,
    last_verified_notes: Option<String>,
    verification_policy_days: i64,
    updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskProjection {
    task: TaskRow,
    owners: Vec<OwnerRow>,
    dependencies: Vec<DependencyRow>,
    tags: Vec<String>,
    verification: Option<VerificationRow>,
}
pub(super) type Projection = BTreeMap<String, TaskProjection>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckpointKind {
    ObservedBaseline,
    Transition,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema_version: u32,
    task_id: String,
    ordinal: u64,
    kind: CheckpointKind,
    predecessor: Option<String>,
    previous_digest: Option<String>,
    source_watermark: u64,
    operation: String,
    actor: String,
    projection: TaskProjection,
    projection_digest: String,
    record_digest: String,
}
impl Checkpoint {
    fn hash(&self) -> Result<String, DecapodError> {
        let mut value = self.clone();
        value.record_digest.clear();
        digest(&value)
    }
}
#[derive(Default)]
struct Journal {
    heads: BTreeMap<String, Checkpoint>,
    history: BTreeMap<String, BTreeSet<String>>,
    records: usize,
}

/// Fail closed on schema additions/removals rather than silently losing columns.
pub(super) fn check_schema(conn: &Connection) -> Result<(), DecapodError> {
    let actual: BTreeSet<String> = conn.columns("tasks")?.into_iter().collect();
    let expected: BTreeSet<String> = [
        "id",
        "hash",
        "title",
        "description",
        "tags",
        "owner",
        "due",
        "ref",
        "status",
        "created_at",
        "updated_at",
        "completed_at",
        "closed_at",
        "dir_path",
        "scope",
        "parent_task_id",
        "priority",
        "depends_on",
        "blocks",
        "category",
        "component",
        "assigned_to",
        "assigned_at",
        "one_shot",
        "lease_expires_at",
        "lease_generation",
        "lease_lifecycle",
        "intent_anchor",
        "revision",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if actual != expected {
        return Err(invalid(
            "unsupported tasks schema; checkpoint migration required",
        ));
    }
    let actual: BTreeSet<String> = conn.columns("task_owners")?.into_iter().collect();
    let expected: BTreeSet<String> = ["id", "task_id", "agent_id", "claimed_at", "claim_type"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if actual != expected {
        return Err(invalid(
            "unsupported task_owners schema; checkpoint migration required",
        ));
    }
    let actual: BTreeSet<String> = conn.columns("task_dependencies")?.into_iter().collect();
    let expected: BTreeSet<String> = ["id", "task_id", "depends_on_task_id", "created_at"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if actual != expected {
        return Err(invalid(
            "unsupported task_dependencies schema; checkpoint migration required",
        ));
    }
    let actual: BTreeSet<String> = conn.columns("task_tags")?.into_iter().collect();
    let expected: BTreeSet<String> = ["task_id", "tag"].into_iter().map(str::to_owned).collect();
    if actual != expected {
        return Err(invalid(
            "unsupported task_tags schema; checkpoint migration required",
        ));
    }
    let actual: BTreeSet<String> = conn.columns("task_verification")?.into_iter().collect();
    let expected: BTreeSet<String> = [
        "todo_id",
        "proof_plan",
        "verification_artifacts",
        "last_verified_at",
        "last_verified_status",
        "last_verified_notes",
        "verification_policy_days",
        "updated_at",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if actual != expected {
        return Err(invalid(
            "unsupported task_verification schema; checkpoint migration required",
        ));
    }
    Ok(())
}

/// Bound source rows/UTF-8 bytes before Dactyl materializes any result rows.
fn projection_size_on_conn(
    conn: &Connection,
    id: &str,
    bytes: &mut usize,
    rows: &mut usize,
) -> Result<(), DecapodError> {
    let (count, size): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(COALESCE(length(CAST(id AS BLOB)),0)+COALESCE(length(CAST(hash AS BLOB)),0)+COALESCE(length(CAST(title AS BLOB)),0)+COALESCE(length(CAST(description AS BLOB)),0)+COALESCE(length(CAST(tags AS BLOB)),0)+COALESCE(length(CAST(owner AS BLOB)),0)+COALESCE(length(CAST(due AS BLOB)),0)+COALESCE(length(CAST(ref AS BLOB)),0)+COALESCE(length(CAST(status AS BLOB)),0)+COALESCE(length(CAST(created_at AS BLOB)),0)+COALESCE(length(CAST(updated_at AS BLOB)),0)+COALESCE(length(CAST(completed_at AS BLOB)),0)+COALESCE(length(CAST(closed_at AS BLOB)),0)+COALESCE(length(CAST(dir_path AS BLOB)),0)+COALESCE(length(CAST(scope AS BLOB)),0)+COALESCE(length(CAST(parent_task_id AS BLOB)),0)+COALESCE(length(CAST(priority AS BLOB)),0)+COALESCE(length(CAST(depends_on AS BLOB)),0)+COALESCE(length(CAST(blocks AS BLOB)),0)+COALESCE(length(CAST(category AS BLOB)),0)+COALESCE(length(CAST(component AS BLOB)),0)+COALESCE(length(CAST(assigned_to AS BLOB)),0)+COALESCE(length(CAST(assigned_at AS BLOB)),0)+COALESCE(length(CAST(one_shot AS BLOB)),0)+COALESCE(length(CAST(lease_expires_at AS BLOB)),0)+COALESCE(length(CAST(lease_generation AS BLOB)),0)+COALESCE(length(CAST(lease_lifecycle AS BLOB)),0)+COALESCE(length(CAST(intent_anchor AS BLOB)),0)+COALESCE(length(CAST(revision AS BLOB)),0)),0) FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    *rows = rows
        .checked_add(usize::try_from(count).map_err(|_| invalid("negative source row count"))?)
        .ok_or_else(|| invalid("source row overflow"))?;
    *bytes = bytes
        .checked_add(usize::try_from(size).map_err(|_| invalid("negative source byte count"))?)
        .ok_or_else(|| invalid("source byte overflow"))?;
    if *rows > MAX_RECORDS || *bytes > MAX_BYTES {
        return Err(invalid("task projection exceeds bounded source size"));
    }
    let (count, size): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(COALESCE(length(CAST(id AS BLOB)),0)+COALESCE(length(CAST(task_id AS BLOB)),0)+COALESCE(length(CAST(agent_id AS BLOB)),0)+COALESCE(length(CAST(claimed_at AS BLOB)),0)+COALESCE(length(CAST(claim_type AS BLOB)),0)),0) FROM task_owners WHERE task_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    *rows = rows
        .checked_add(usize::try_from(count).map_err(|_| invalid("negative source row count"))?)
        .ok_or_else(|| invalid("source row overflow"))?;
    *bytes = bytes
        .checked_add(usize::try_from(size).map_err(|_| invalid("negative source byte count"))?)
        .ok_or_else(|| invalid("source byte overflow"))?;
    if *rows > MAX_RECORDS || *bytes > MAX_BYTES {
        return Err(invalid("task projection exceeds bounded source size"));
    }
    let (count, size): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(COALESCE(length(CAST(id AS BLOB)),0)+COALESCE(length(CAST(task_id AS BLOB)),0)+COALESCE(length(CAST(depends_on_task_id AS BLOB)),0)+COALESCE(length(CAST(created_at AS BLOB)),0)),0) FROM task_dependencies WHERE task_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    *rows = rows
        .checked_add(usize::try_from(count).map_err(|_| invalid("negative source row count"))?)
        .ok_or_else(|| invalid("source row overflow"))?;
    *bytes = bytes
        .checked_add(usize::try_from(size).map_err(|_| invalid("negative source byte count"))?)
        .ok_or_else(|| invalid("source byte overflow"))?;
    if *rows > MAX_RECORDS || *bytes > MAX_BYTES {
        return Err(invalid("task projection exceeds bounded source size"));
    }
    let (count, size): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(COALESCE(length(CAST(task_id AS BLOB)),0)+COALESCE(length(CAST(tag AS BLOB)),0)),0) FROM task_tags WHERE task_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    *rows = rows
        .checked_add(usize::try_from(count).map_err(|_| invalid("negative source row count"))?)
        .ok_or_else(|| invalid("source row overflow"))?;
    *bytes = bytes
        .checked_add(usize::try_from(size).map_err(|_| invalid("negative source byte count"))?)
        .ok_or_else(|| invalid("source byte overflow"))?;
    if *rows > MAX_RECORDS || *bytes > MAX_BYTES {
        return Err(invalid("task projection exceeds bounded source size"));
    }
    let (count, size): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(COALESCE(length(CAST(todo_id AS BLOB)),0)+COALESCE(length(CAST(proof_plan AS BLOB)),0)+COALESCE(length(CAST(verification_artifacts AS BLOB)),0)+COALESCE(length(CAST(last_verified_at AS BLOB)),0)+COALESCE(length(CAST(last_verified_status AS BLOB)),0)+COALESCE(length(CAST(last_verified_notes AS BLOB)),0)+COALESCE(length(CAST(verification_policy_days AS BLOB)),0)+COALESCE(length(CAST(updated_at AS BLOB)),0)),0) FROM task_verification WHERE todo_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    *rows = rows
        .checked_add(usize::try_from(count).map_err(|_| invalid("negative source row count"))?)
        .ok_or_else(|| invalid("source row overflow"))?;
    *bytes = bytes
        .checked_add(usize::try_from(size).map_err(|_| invalid("negative source byte count"))?)
        .ok_or_else(|| invalid("source byte overflow"))?;
    if *rows > MAX_RECORDS || *bytes > MAX_BYTES {
        return Err(invalid("task projection exceeds bounded source size"));
    }
    Ok(())
}

pub(super) fn capture(conn: &Connection) -> Result<Projection, DecapodError> {
    check_schema(conn)?;
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM task_owners c LEFT JOIN tasks t ON t.id=c.task_id WHERE t.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM task_dependencies c LEFT JOIN tasks t ON t.id=c.task_id WHERE t.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM task_dependencies c LEFT JOIN tasks t ON t.id=c.depends_on_task_id WHERE t.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM task_tags c LEFT JOIN tasks t ON t.id=c.task_id WHERE t.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM task_verification c LEFT JOIN tasks t ON t.id=c.todo_id WHERE t.id IS NULL)",
    ] {
        if conn.query_row(sql, [], |r| r.get::<_, bool>(0))? {
            return Err(invalid(
                "orphan task projection rows require explicit recovery",
            ));
        }
    }
    let (count, bytes): (i64, i64) = conn.query_row(
        "SELECT count(*),COALESCE(sum(length(CAST(id AS BLOB))),0) FROM tasks",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if count > MAX_TASKS as i64 || bytes > MAX_BYTES as i64 {
        return Err(invalid("task identities exceed bounded checkpoint limit"));
    }
    let mut stmt = conn.prepare("SELECT id FROM tasks ORDER BY id LIMIT ?1")?;
    let rows = stmt.query_map(params![(MAX_TASKS + 1) as i64], |row| {
        row.get::<_, String>(0)
    })?;
    let ids = rows.collect::<Result<Vec<_>, _>>()?;
    capture_selected(conn, &ids)
}

pub(super) fn capture_selected(
    conn: &Connection,
    ids: &[String],
) -> Result<Projection, DecapodError> {
    check_schema(conn)?;
    if ids.len() > MAX_TASKS {
        return Err(invalid("task count exceeds bounded checkpoint limit"));
    }
    let mut out = BTreeMap::new();
    let mut source_bytes = 0usize;
    let mut source_rows = 0usize;
    for selected_id in ids {
        projection_size_on_conn(conn, selected_id, &mut source_bytes, &mut source_rows)?;
        let mut stmt = conn.prepare("SELECT id,hash,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at,one_shot,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision FROM tasks WHERE id=?1 ORDER BY id")?;
        let rows = stmt.query_map([selected_id], |row| {
            Ok(TaskRow {
                id: row.get(0)?,
                hash: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                tags: row.get(4)?,
                owner: row.get(5)?,
                due: row.get(6)?,
                r#ref: row.get(7)?,
                status: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                completed_at: row.get(11)?,
                closed_at: row.get(12)?,
                dir_path: row.get(13)?,
                scope: row.get(14)?,
                parent_task_id: row.get(15)?,
                priority: row.get(16)?,
                depends_on: row.get(17)?,
                blocks: row.get(18)?,
                category: row.get(19)?,
                component: row.get(20)?,
                assigned_to: row.get(21)?,
                assigned_at: row.get(22)?,
                one_shot: row.get(23)?,
                lease_expires_at: row.get(24)?,
                lease_generation: row.get(25)?,
                lease_lifecycle: row.get(26)?,
                intent_anchor: row.get(27)?,
                revision: row.get(28)?,
            })
        })?;
        for row in rows {
            let task = row?;
            if task.revision < 0 || task.lease_generation.unwrap_or(0) < 0 {
                return Err(invalid("negative task revision or lease generation"));
            }
            out.insert(
                task.id.clone(),
                TaskProjection {
                    task,
                    owners: vec![],
                    dependencies: vec![],
                    tags: vec![],
                    verification: None,
                },
            );
        }
        let mut stmt = conn.prepare("SELECT task_id,id,agent_id,claimed_at,claim_type FROM task_owners WHERE task_id=?1 ORDER BY task_id,agent_id,id")?;
        let rows = stmt.query_map([selected_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                OwnerRow {
                    id: row.get(1)?,
                    agent_id: row.get(2)?,
                    claimed_at: row.get(3)?,
                    claim_type: row.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (id, value) = row?;
            out.get_mut(&id)
                .ok_or_else(|| invalid("orphan task owner"))?
                .owners
                .push(value);
        }
        let mut stmt = conn.prepare("SELECT task_id,id,depends_on_task_id,created_at FROM task_dependencies WHERE task_id=?1 ORDER BY task_id,depends_on_task_id,id")?;
        let rows = stmt.query_map([selected_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                DependencyRow {
                    id: row.get(1)?,
                    depends_on_task_id: row.get(2)?,
                    created_at: row.get(3)?,
                },
            ))
        })?;
        for row in rows {
            let (id, value) = row?;
            out.get_mut(&id)
                .ok_or_else(|| invalid("orphan task dependency"))?
                .dependencies
                .push(value);
        }
        let mut stmt = conn
            .prepare("SELECT task_id,tag FROM task_tags WHERE task_id=?1 ORDER BY task_id,tag")?;
        let rows = stmt.query_map([selected_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, value) = row?;
            out.get_mut(&id)
                .ok_or_else(|| invalid("orphan task tag"))?
                .tags
                .push(value);
        }
        let mut stmt = conn.prepare("SELECT todo_id,proof_plan,verification_artifacts,last_verified_at,last_verified_status,last_verified_notes,verification_policy_days,updated_at FROM task_verification WHERE todo_id=?1 ORDER BY todo_id")?;
        let rows = stmt.query_map([selected_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                VerificationRow {
                    proof_plan: row.get(1)?,
                    verification_artifacts: row.get(2)?,
                    last_verified_at: row.get(3)?,
                    last_verified_status: row.get(4)?,
                    last_verified_notes: row.get(5)?,
                    verification_policy_days: row.get(6)?,
                    updated_at: row.get(7)?,
                },
            ))
        })?;
        for row in rows {
            let (id, value) = row?;
            out.get_mut(&id)
                .ok_or_else(|| invalid("orphan task proof"))?
                .verification = Some(value);
        }
    }
    if serde_json::to_vec(&out)
        .map_err(|e| invalid(e.to_string()))?
        .len()
        > MAX_BYTES
    {
        return Err(invalid("task snapshot exceeds bounded byte limit"));
    }
    Ok(out)
}

pub(super) fn watermark(conn: &Connection) -> Result<u64, DecapodError> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(seq),0) FROM events WHERE stream='todo'",
        [],
        |r| r.get(0),
    )?)
}

fn validate_projection(value: &TaskProjection) -> Result<(), DecapodError> {
    if value.task.id.is_empty()
        || value.task.revision < 0
        || value.task.lease_generation.unwrap_or(0) < 0
    {
        return Err(invalid("invalid task identity/revision/generation"));
    }
    let owner_keys: Vec<_> = value.owners.iter().map(|o| (&o.agent_id, &o.id)).collect();
    let mut sorted = owner_keys.clone();
    sorted.sort();
    if sorted != owner_keys
        || value
            .owners
            .iter()
            .map(|o| &o.agent_id)
            .collect::<BTreeSet<_>>()
            .len()
            != value.owners.len()
    {
        return Err(invalid("noncanonical or duplicate owners"));
    }
    let dependency_keys: Vec<_> = value
        .dependencies
        .iter()
        .map(|d| (&d.depends_on_task_id, &d.id))
        .collect();
    let mut sorted = dependency_keys.clone();
    sorted.sort();
    if sorted != dependency_keys
        || value
            .dependencies
            .iter()
            .map(|d| &d.depends_on_task_id)
            .collect::<BTreeSet<_>>()
            .len()
            != value.dependencies.len()
    {
        return Err(invalid("noncanonical or duplicate dependencies"));
    }
    if value
        .tags
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        != value.tags.iter().collect::<Vec<_>>()
    {
        return Err(invalid("noncanonical or duplicate tags"));
    }
    Ok(())
}

struct CheckpointEvent {
    event: events::StoredEvent,
    subject_id: Option<String>,
    subject_kind: Option<String>,
}
fn checkpoint_events(
    conn: &Connection,
    ids: Option<&[String]>,
) -> Result<Vec<CheckpointEvent>, DecapodError> {
    let mut out = Vec::new();
    let mut total_bytes = 0usize;
    let mut total_records = 0usize;
    let selections: Vec<Option<&str>> = match ids {
        Some(ids) => ids.iter().map(|id| Some(id.as_str())).collect(),
        None => vec![None],
    };
    for id in selections {
        // Separate fixed SQL shapes retain the subject index for task-local calls.
        let (size_sql, rows_sql) = if id.is_some() {
            (
                "SELECT count(*), COALESCE(sum(length(CAST(payload AS BLOB))+length(CAST(event_id AS BLOB))+length(CAST(ts AS BLOB))+length(CAST(actor AS BLOB))),0), COALESCE(max(length(CAST(payload AS BLOB))),0) FROM events WHERE stream='todo' AND event_type=?1 AND subject_id=?2",
                "SELECT event_id,ts,seq,event_type,payload,actor,subject_id,subject_kind FROM events WHERE stream='todo' AND event_type=?1 AND subject_id=?2 ORDER BY seq,event_id LIMIT ?3",
            )
        } else {
            (
                "SELECT count(*), COALESCE(sum(length(CAST(payload AS BLOB))+length(CAST(event_id AS BLOB))+length(CAST(ts AS BLOB))+length(CAST(actor AS BLOB))),0), COALESCE(max(length(CAST(payload AS BLOB))),0) FROM events WHERE stream='todo' AND event_type=?1 AND ?2 IS NULL",
                "SELECT event_id,ts,seq,event_type,payload,actor,subject_id,subject_kind FROM events WHERE stream='todo' AND event_type=?1 AND ?2 IS NULL ORDER BY seq,event_id LIMIT ?3",
            )
        };
        let (count, size, largest): (i64, i64, i64) =
            conn.query_row(size_sql, params![EVENT_TYPE, id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        total_records = total_records
            .checked_add(usize::try_from(count).map_err(|_| invalid("negative journal count"))?)
            .ok_or_else(|| invalid("journal count overflow"))?;
        total_bytes = total_bytes
            .checked_add(usize::try_from(size).map_err(|_| invalid("negative journal byte count"))?)
            .ok_or_else(|| invalid("journal byte overflow"))?;
        if total_records > MAX_RECORDS
            || total_bytes > MAX_BYTES
            || largest > MAX_RECORD_BYTES as i64
        {
            return Err(invalid(
                "journal exceeds bounded replay limit; explicit checkpoint migration required",
            ));
        }
        let mut stmt = conn.prepare(rows_sql)?;
        let rows = stmt.query_map(params![EVENT_TYPE, id, (MAX_RECORDS + 1) as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        for row in rows {
            let (event_id, ts, seq, event_type, payload, actor, subject_id, subject_kind) = row?;
            out.push(CheckpointEvent {
                event: events::StoredEvent {
                    stream: events::TODO.into(),
                    event_id,
                    ts,
                    seq,
                    event_type,
                    payload: serde_json::from_str(&payload)
                        .map_err(|e| invalid(format!("corrupt checkpoint JSON: {e}")))?,
                    actor,
                },
                subject_id,
                subject_kind,
            });
        }
    }
    out.sort_by(|a, b| {
        a.event
            .seq
            .cmp(&b.event.seq)
            .then(a.event.event_id.cmp(&b.event.event_id))
    });
    Ok(out)
}

fn read_journal(conn: &Connection, ids: Option<&[String]>) -> Result<Journal, DecapodError> {
    journal_from_records(checkpoint_events(conn, ids)?)
}

fn journal_from_records(records: Vec<CheckpointEvent>) -> Result<Journal, DecapodError> {
    let mut journal = Journal::default();
    // The events owner validates the canonical row/envelope contract. Historical
    // non-checkpoint events are not interpreted as task state.
    for record in records {
        let event = record.event;
        if event.event_type != EVENT_TYPE {
            continue;
        }
        let envelope = &event.payload;
        if envelope.get("event_type").and_then(|v| v.as_str()) != Some(EVENT_TYPE)
            || envelope.get("event_id").and_then(|v| v.as_str()) != Some(event.event_id.as_str())
            || envelope.get("actor").and_then(|v| v.as_str()) != Some(event.actor.as_str())
            || envelope.get("ts").and_then(|v| v.as_str()) != Some(event.ts.as_str())
            || record.subject_kind.as_deref() != Some("task")
            || envelope.get("task_id").and_then(|v| v.as_str()) != record.subject_id.as_deref()
            || envelope.get("status").and_then(|v| v.as_str()) != Some("success")
        {
            return Err(invalid("checkpoint event envelope mismatch"));
        }
        let checkpoint: Checkpoint = serde_json::from_value(
            envelope
                .get("payload")
                .cloned()
                .ok_or_else(|| invalid("missing checkpoint payload"))?,
        )
        .map_err(|e| invalid(format!("invalid checkpoint: {e}")))?;
        validate_projection(&checkpoint.projection)?;
        if checkpoint.schema_version != SCHEMA_VERSION
            || checkpoint.task_id != checkpoint.projection.task.id
            || envelope.get("task_id").and_then(|v| v.as_str()) != Some(checkpoint.task_id.as_str())
            || checkpoint.actor != event.actor
            || checkpoint.projection_digest != digest(&checkpoint.projection)?
            || checkpoint.record_digest != checkpoint.hash()?
            || event.event_id != format!("task-journal:{}", checkpoint.record_digest)
            || checkpoint.source_watermark >= event.seq
        {
            return Err(invalid(
                "checkpoint identity/schema/digest/watermark mismatch",
            ));
        }
        match journal.heads.get(&checkpoint.task_id) {
            None => {
                if checkpoint.kind != CheckpointKind::ObservedBaseline
                    || checkpoint.ordinal != 1
                    || checkpoint.predecessor.is_some()
                    || checkpoint.previous_digest.is_some()
                {
                    return Err(invalid("checkpoint chain missing observed baseline"));
                }
            }
            Some(previous) => {
                if checkpoint.kind != CheckpointKind::Transition
                    || checkpoint.ordinal
                        != previous
                            .ordinal
                            .checked_add(1)
                            .ok_or_else(|| invalid("checkpoint ordinal overflow"))?
                    || checkpoint.predecessor.as_deref() != Some(previous.record_digest.as_str())
                    || checkpoint.previous_digest.as_deref()
                        != Some(previous.projection_digest.as_str())
                    || checkpoint.projection.task.revision < previous.projection.task.revision
                    || checkpoint.projection.task.lease_generation.unwrap_or(0)
                        < previous.projection.task.lease_generation.unwrap_or(0)
                    || checkpoint.source_watermark < previous.source_watermark
                {
                    return Err(invalid(format!(
                        "contradictory, truncated or regressing chain for {}",
                        checkpoint.task_id
                    )));
                }
            }
        }
        journal
            .history
            .entry(checkpoint.task_id.clone())
            .or_default()
            .insert(checkpoint.projection_digest.clone());
        journal.heads.insert(checkpoint.task_id.clone(), checkpoint);
        journal.records += 1;
    }
    Ok(journal)
}

/// A surviving domain mutation after the last complete snapshot means the tail
/// is incomplete. Never replay an older snapshot over that evidence. Unknown
/// subject events fail closed; only known read-only/audit events are exempt.
fn check_complete_suffix(
    conn: &Connection,
    journal: &Journal,
    through: u64,
) -> Result<(), DecapodError> {
    for (id, head) in &journal.heads {
        let incomplete: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND subject_id=?1 AND seq>?2 AND seq<=?3 AND event_type NOT IN ('task.journal.checkpoint','task.journal.restored','task.comment','task.worker.run','task.verify.capture_failed'))",
            params![id,head.source_watermark,through], |r| r.get(0),
        )?;
        if incomplete {
            return Err(invalid(format!(
                "TASK_JOURNAL_INCOMPLETE_HISTORY: uncheckpointed domain-event suffix for {id}"
            )));
        }
    }
    Ok(())
}

fn append_checkpoint(
    conn: &Connection,
    journal: &mut Journal,
    projection: &TaskProjection,
    source_watermark: u64,
    operation: &str,
    actor: &str,
) -> Result<(), DecapodError> {
    require_transaction(conn)?;
    validate_projection(projection)?;
    let previous = journal.heads.get(&projection.task.id);
    if let Some(previous) = previous
        && (projection.task.revision < previous.projection.task.revision
            || projection.task.lease_generation.unwrap_or(0)
                < previous.projection.task.lease_generation.unwrap_or(0))
    {
        return Err(invalid("task revision or lease generation regressed"));
    }
    let mut checkpoint = Checkpoint {
        schema_version: SCHEMA_VERSION,
        task_id: projection.task.id.clone(),
        ordinal: previous
            .map(|p| {
                p.ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid("checkpoint ordinal overflow"))
            })
            .transpose()?
            .unwrap_or(1),
        kind: if previous.is_some() {
            CheckpointKind::Transition
        } else {
            CheckpointKind::ObservedBaseline
        },
        predecessor: previous.map(|p| p.record_digest.clone()),
        previous_digest: previous.map(|p| p.projection_digest.clone()),
        source_watermark,
        operation: operation.to_owned(),
        actor: actor.to_owned(),
        projection: projection.clone(),
        projection_digest: digest(projection)?,
        record_digest: String::new(),
    };
    checkpoint.record_digest = checkpoint.hash()?;
    let envelope = serde_json::json!({
        "ts": super::now_iso(), "event_id": format!("task-journal:{}", checkpoint.record_digest),
        "event_type": EVENT_TYPE, "status": "success", "task_id": checkpoint.task_id,
        "actor": actor, "payload": checkpoint,
    });
    let added_bytes = serde_json::to_vec(&envelope)
        .map_err(|e| invalid(e.to_string()))?
        .len();
    let (count, bytes): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(length(CAST(payload AS BLOB))+length(CAST(event_id AS BLOB))+length(CAST(ts AS BLOB))+length(CAST(actor AS BLOB))),0) FROM events WHERE stream='todo' AND event_type=?1 AND subject_id=?2", params![EVENT_TYPE,checkpoint.task_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    // Metadata is bounded by this generated envelope too. Refuse before committing
    // a new checkpoint that would make the subject unreplayable within its limit.
    if added_bytes > MAX_RECORD_BYTES
        || count >= MAX_RECORDS as i64
        || bytes
            .checked_add((added_bytes * 2) as i64)
            .is_none_or(|size| size > MAX_BYTES as i64)
    {
        return Err(invalid(
            "prospective task journal exceeds bounded replay limit; explicit checkpoint migration required",
        ));
    }
    events::append_on_conn(conn, events::TODO, &envelope)?;
    journal
        .history
        .entry(checkpoint.task_id.clone())
        .or_default()
        .insert(checkpoint.projection_digest.clone());
    journal.heads.insert(checkpoint.task_id.clone(), checkpoint);
    journal.records += 1;
    Ok(())
}

/// Capture the before image on the transaction connection, then call this after
/// the existing mutation/event writes. A conflict rolls back both domain and journal.
pub(super) fn record_changes(
    conn: &Connection,
    before: &Projection,
    ids: &[String],
    before_watermark: u64,
    operation: &str,
    actor: &str,
) -> Result<(), DecapodError> {
    require_transaction(conn)?;
    let after = capture_selected(conn, ids)?;
    let source_watermark = watermark(conn)?;
    let mut journal = read_journal(conn, Some(ids))?;
    check_complete_suffix(conn, &journal, before_watermark)?;
    for id in before.keys() {
        if !after.contains_key(id) {
            return Err(invalid(
                "task deletion requires a versioned tombstone contract",
            ));
        }
    }
    for (id, current) in &after {
        let prior = before.get(id);
        if prior == Some(current) {
            let has_domain_event: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND subject_id=?1 AND seq>?2 AND event_type NOT IN ('task.journal.checkpoint','task.journal.restored','task.comment','task.worker.run','task.verify.capture_failed'))",
                params![id,before_watermark], |r| r.get(0),
            )?;
            if !has_domain_event {
                continue;
            }
        }
        if let Some(head) = journal.heads.get(id) {
            if prior.map(digest).transpose()?.as_deref() != Some(head.projection_digest.as_str()) {
                return Err(invalid(format!(
                    "live task {id} diverged from journal; inspect before recovery"
                )));
            }
        } else if let Some(prior) = prior {
            append_checkpoint(
                conn,
                &mut journal,
                prior,
                before_watermark,
                "observed_before_mutation",
                actor,
            )?;
        }
        // New task creation is explicitly observed current state, never a replay
        // claim that historical pre-checkpoint events were complete.
        append_checkpoint(
            conn,
            &mut journal,
            current,
            source_watermark,
            operation,
            actor,
        )?;
    }
    Ok(())
}

fn report(conn: &Connection, journal: &Journal) -> Result<serde_json::Value, DecapodError> {
    let current = capture(conn)?;
    let mut divergent = Vec::new();
    let mut missing = Vec::new();
    for (id, head) in &journal.heads {
        match current.get(id) {
            None => missing.push(id.clone()),
            Some(value) if digest(value)? != head.projection_digest => divergent.push(id.clone()),
            _ => (),
        }
    }
    let uncheckpointed: Vec<_> = current
        .keys()
        .filter(|id| !journal.heads.contains_key(*id))
        .cloned()
        .collect();
    Ok(serde_json::json!({
        "schema_version": "decapod.task-journal.v1",
        "status": if divergent.is_empty() && missing.is_empty() && uncheckpointed.is_empty() { "ok" } else { "conflict" },
        "journal_valid": true, "records": journal.records, "checkpointed_tasks": journal.heads.len(),
        "current_digest": digest(&current)?, "divergent_tasks": divergent, "missing_tasks": missing,
        "uncheckpointed_tasks": uncheckpointed,
        "history_origin": "observed_current_state_baselines", "proof_policy": "preserve_bytes_no_refresh",
    }))
}

/// Read-only consistency for execution custody. Legacy subjects without any
/// modern checkpoint remain explicit observations; a known journal must agree.
pub(super) fn check_execution_authority(
    conn: &Connection,
    task_id: &str,
) -> Result<(), DecapodError> {
    transfer::ensure_not_exported(conn, task_id)?;
    use crate::core::db::OptionalExtension;
    let raw: Option<String> = conn
        .query_row(
            "SELECT COALESCE(depends_on,'') FROM tasks WHERE id=?1",
            [task_id],
            |r| r.get(0),
        )
        .optional()?;
    let mut ids = vec![task_id.to_owned()];
    if let Some(raw) = raw {
        ids.extend(super::parse_dependency_ids(&raw));
    }
    ids.sort();
    ids.dedup();
    let current = capture_selected(conn, &ids)?;
    let journal = read_journal(conn, Some(&ids))?;
    check_complete_suffix(conn, &journal, watermark(conn)?)?;
    for (id, head) in &journal.heads {
        if current.get(id).map(digest).transpose()?.as_deref()
            != Some(head.projection_digest.as_str())
        {
            return Err(invalid(format!(
                "execution custody cannot bind divergent journaled authority for {id}"
            )));
        }
    }
    Ok(())
}

pub(super) fn verify(conn: &Connection) -> Result<serde_json::Value, DecapodError> {
    check_schema(conn)?;
    let journal = read_journal(conn, None)?;
    check_complete_suffix(conn, &journal, watermark(conn)?)?;
    report(conn, &journal)
}

pub(super) fn checkpoint(
    conn: &Connection,
    actor: &str,
) -> Result<serde_json::Value, DecapodError> {
    require_transaction(conn)?;
    let current = capture(conn)?;
    let source_watermark = watermark(conn)?;
    let mut journal = read_journal(conn, None)?;
    check_complete_suffix(conn, &journal, source_watermark)?;
    for (id, head) in &journal.heads {
        if current.get(id).map(digest).transpose()?.as_deref()
            != Some(head.projection_digest.as_str())
        {
            return Err(invalid(
                "baseline cannot replace divergent or missing journaled state",
            ));
        }
    }
    for (id, projection) in &current {
        if !journal.heads.contains_key(id) {
            append_checkpoint(
                conn,
                &mut journal,
                projection,
                source_watermark,
                "explicit_current_state_baseline",
                actor,
            )?;
        }
    }
    report(conn, &journal)
}

/// Restore only exact journal predecessors or absent rows, never arbitrary live
/// state. All preconditions are checked before the first write in the transaction.
pub(super) fn restore(
    conn: &Connection,
    expected_current_digest: &str,
) -> Result<serde_json::Value, DecapodError> {
    require_transaction(conn)?;
    let current = capture(conn)?;
    if expected_current_digest.len() != 64 || expected_current_digest != digest(&current)? {
        return Err(invalid("current projection changed since recovery review"));
    }
    let journal = read_journal(conn, None)?;
    check_complete_suffix(conn, &journal, watermark(conn)?)?;
    let mut restored = Vec::new();
    for (id, head) in &journal.heads {
        if let Some(live) = current.get(id) {
            let hash = digest(live)?;
            if hash == head.projection_digest {
                continue;
            }
            if !journal
                .history
                .get(id)
                .is_some_and(|known| known.contains(&hash))
            {
                return Err(invalid(format!(
                    "refusing to overwrite divergent live authority for {id}"
                )));
            }
        }
        restored.push(id.clone());
    }
    let final_ids: BTreeSet<_> = current.keys().chain(journal.heads.keys()).collect();
    for head in journal.heads.values() {
        for dependency in &head.projection.dependencies {
            if !final_ids.contains(&dependency.depends_on_task_id) {
                return Err(invalid("checkpoint dependency target is missing"));
            }
        }
    }
    // Upsert parents first; never REPLACE (which cascades unrelated child rows).
    for id in &restored {
        write_task(conn, &journal.heads[id].projection.task)?;
    }
    for id in &restored {
        write_children(conn, &journal.heads[id].projection)?;
    }
    let mut result = report(conn, &journal)?;
    result["restored_tasks"] = serde_json::json!(restored);
    Ok(result)
}

fn write_task(conn: &Connection, task: &TaskRow) -> Result<(), DecapodError> {
    conn.execute("INSERT INTO tasks (id,hash,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at,one_shot,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29) ON CONFLICT(id) DO UPDATE SET hash=excluded.hash,title=excluded.title,description=excluded.description,tags=excluded.tags,owner=excluded.owner,due=excluded.due,ref=excluded.ref,status=excluded.status,created_at=excluded.created_at,updated_at=excluded.updated_at,completed_at=excluded.completed_at,closed_at=excluded.closed_at,dir_path=excluded.dir_path,scope=excluded.scope,parent_task_id=excluded.parent_task_id,priority=excluded.priority,depends_on=excluded.depends_on,blocks=excluded.blocks,category=excluded.category,component=excluded.component,assigned_to=excluded.assigned_to,assigned_at=excluded.assigned_at,one_shot=excluded.one_shot,lease_expires_at=excluded.lease_expires_at,lease_generation=excluded.lease_generation,lease_lifecycle=excluded.lease_lifecycle,intent_anchor=excluded.intent_anchor,revision=excluded.revision", params![
        task.id,
        task.hash,
        task.title,
        task.description,
        task.tags,
        task.owner,
        task.due,
        task.r#ref,
        task.status,
        task.created_at,
        task.updated_at,
        task.completed_at,
        task.closed_at,
        task.dir_path,
        task.scope,
        task.parent_task_id,
        task.priority,
        task.depends_on,
        task.blocks,
        task.category,
        task.component,
        task.assigned_to,
        task.assigned_at,
        task.one_shot,
        task.lease_expires_at,
        task.lease_generation,
        task.lease_lifecycle,
        task.intent_anchor,
        task.revision,
    ])?;
    Ok(())
}
fn write_children(conn: &Connection, projection: &TaskProjection) -> Result<(), DecapodError> {
    let id = &projection.task.id;
    conn.execute("DELETE FROM task_owners WHERE task_id=?1", [id])?;
    conn.execute("DELETE FROM task_dependencies WHERE task_id=?1", [id])?;
    conn.execute("DELETE FROM task_tags WHERE task_id=?1", [id])?;
    conn.execute("DELETE FROM task_verification WHERE todo_id=?1", [id])?;
    for value in &projection.owners {
        conn.execute("INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type) VALUES(?1,?2,?3,?4,?5)", params![value.id,id,value.agent_id,value.claimed_at,value.claim_type])?;
    }
    for value in &projection.dependencies {
        conn.execute("INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at) VALUES(?1,?2,?3,?4)", params![value.id,id,value.depends_on_task_id,value.created_at])?;
    }
    for tag in &projection.tags {
        conn.execute(
            "INSERT INTO task_tags(task_id,tag) VALUES(?1,?2)",
            params![id, tag],
        )?;
    }
    if let Some(value) = &projection.verification {
        conn.execute("INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_at,last_verified_status,last_verified_notes,verification_policy_days,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![id,value.proof_plan,value.verification_artifacts,value.last_verified_at,value.last_verified_status,value.last_verified_notes,value.verification_policy_days,value.updated_at])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/task_journal_tests.rs"]
mod tests;
