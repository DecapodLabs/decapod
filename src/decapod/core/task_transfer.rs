//! Explicit, operator-trusted transfer of a standalone local task.
//!
//! Source history is retained inside one canonical TODO event, never replayed
//! as locally observed history. A yielded source is sealed before file publication.
//! This cooperates with the local journal; it cannot police disconnected clones.
use super::super::{
    TaskJournalSelection, TodoTransferCommand, enforce_operation_policy, now_iso,
    repo_root_from_store_root, todo_db_path, with_task_journal,
};
use super::*;
use crate::core::{atomic, broker::DbBroker, fs_permissions, repo_identity};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "decapod.task-transfer.v1";
const EXPORTED: &str = "task.transfer.exported";
const IMPORTED: &str = "task.transfer.imported";
const MAX_PACKAGE: usize = 4 * 1024 * 1024;
const MAX_HISTORY: usize = 4096;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalEvent {
    stream: String,
    event_id: String,
    ts: String,
    seq: u64,
    event_type: String,
    payload: Value,
    actor: String,
}
impl From<events::StoredEvent> for HistoricalEvent {
    fn from(e: events::StoredEvent) -> Self {
        Self {
            stream: e.stream,
            event_id: e.event_id,
            ts: e.ts,
            seq: e.seq,
            event_type: e.event_type,
            payload: e.payload,
            actor: e.actor,
        }
    }
}
impl HistoricalEvent {
    fn stored(&self) -> events::StoredEvent {
        events::StoredEvent {
            stream: self.stream.clone(),
            event_id: self.event_id.clone(),
            ts: self.ts.clone(),
            seq: self.seq,
            event_type: self.event_type.clone(),
            payload: self.payload.clone(),
            actor: self.actor.clone(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: String,
    transfer_id: String,
    repository: String,
    source_commit: String,
    source_branch: String,
    exporter: String,
    receiver: String,
    exported_at: String,
    source_watermark: u64,
    source_head: String,
    sealed_revision: i64,
    projection: TaskProjection,
    history: Vec<HistoricalEvent>,
}

fn fail(message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("TASK_TRANSFER_REJECTED: {message}"))
}
fn actor_valid(actor: &str) -> bool {
    !actor.is_empty()
        && actor.len() <= 128
        && actor
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn sha(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn encode(bundle: &Bundle) -> Result<Vec<u8>, DecapodError> {
    let bytes =
        serde_json::to_vec_pretty(bundle).map_err(|_| fail("artifact serialization failed"))?;
    if bytes.len() > MAX_PACKAGE {
        return Err(fail(
            "artifact exceeds 4 MiB; no partial history can be transferred",
        ));
    }
    Ok(bytes)
}

/// Called in the existing task mutation transaction, before any domain changes.
pub(in super::super) fn ensure_not_exported(
    conn: &Connection,
    id: &str,
) -> Result<(), DecapodError> {
    let sealed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND subject_id=?1 AND event_type=?2)",
        params![id, EXPORTED], |r| r.get(0))?;
    if sealed {
        return Err(fail(
            "source task is sealed for transfer; recover its artifact, do not reclaim it",
        ));
    }
    Ok(())
}

fn standalone(projection: &TaskProjection) -> Result<(), DecapodError> {
    let t = &projection.task;
    validate_projection(projection)?;
    if !matches!(t.status.as_str(), "open" | "in_progress")
        || t.lease_lifecycle.as_deref() != Some("yielded")
        || t.assigned_to.as_deref().is_some_and(|s| !s.is_empty())
        || t.assigned_at.is_some()
        || t.lease_expires_at.is_some()
        || t.lease_generation.unwrap_or(0) <= 0
        || !projection.owners.is_empty()
        || t.intent_anchor.as_deref().is_none_or(str::is_empty)
    {
        return Err(fail(
            "task must be explicitly yielded with no remaining owners or lease",
        ));
    }
    if t.parent_task_id.is_some()
        || !projection.dependencies.is_empty()
        || t.depends_on.as_deref().is_some_and(|s| !s.is_empty())
        || t.blocks.as_deref().is_some_and(|s| !s.is_empty())
    {
        return Err(fail(
            "only standalone tasks are supported; connected custody needs a reviewed transfer closure",
        ));
    }
    Ok(())
}

fn validate(bundle: &Bundle) -> Result<(), DecapodError> {
    standalone(&bundle.projection)?;
    if bundle.schema != SCHEMA
        || !actor_valid(&bundle.exporter)
        || !actor_valid(&bundle.receiver)
        || bundle.exporter == bundle.receiver
        || bundle.transfer_id.len() != 26
        || !bundle
            .transfer_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric())
        || bundle.source_commit.len() != 40
        || !bundle.source_commit.bytes().all(|b| b.is_ascii_hexdigit())
        || bundle.source_branch.is_empty()
        || bundle.source_branch.len() > 512
        || bundle.repository.is_empty()
        || bundle.repository.len() > 256
        || bundle.history.is_empty()
        || bundle.history.len() > MAX_HISTORY
        || Some(bundle.sealed_revision) != bundle.projection.task.revision.checked_add(1)
    {
        return Err(fail("invalid schema, custody binding or source revision"));
    }
    let id = &bundle.projection.task.id;
    let mut sequence = 0;
    let mut event_ids = BTreeSet::new();
    let mut checkpoints = Vec::new();
    let mut last_domain = 0;
    let mut last_checkpoint = 0;
    let mut yielded_by = None;
    for e in &bundle.history {
        if e.stream != events::TODO
            || e.seq <= sequence
            || e.seq > bundle.source_watermark
            || !event_ids.insert(&e.event_id)
            || e.payload.get("task_id").and_then(Value::as_str) != Some(id.as_str())
            || e.payload.get("event_id").and_then(Value::as_str) != Some(e.event_id.as_str())
            || e.payload.get("event_type").and_then(Value::as_str) != Some(e.event_type.as_str())
            || e.payload.get("actor").and_then(Value::as_str) != Some(e.actor.as_str())
            || e.payload.get("ts").and_then(Value::as_str) != Some(e.ts.as_str())
        {
            return Err(fail("history identity, order or envelope mismatch"));
        }
        sequence = e.seq;
        if e.event_type == EVENT_TYPE {
            last_checkpoint = e.seq;
            checkpoints.push(CheckpointEvent {
                event: e.stored(),
                subject_id: Some(id.clone()),
                subject_kind: Some("task".into()),
            });
        } else if !matches!(
            e.event_type.as_str(),
            "task.comment"
                | "task.worker.run"
                | "task.verify.capture_failed"
                | "task.journal.restored"
        ) {
            last_domain = e.seq;
            if e.event_type == "task.yield" {
                yielded_by = Some(e.actor.as_str());
            }
        }
    }
    let journal = journal_from_records(checkpoints)?;
    let head = journal
        .heads
        .get(id)
        .ok_or_else(|| fail("source checkpoint chain is missing"))?;
    if journal.heads.len() != 1
        || head.record_digest != bundle.source_head
        || head.projection != bundle.projection
        || last_domain > last_checkpoint
        || yielded_by != Some(bundle.exporter.as_str())
    {
        return Err(fail(
            "source history is incomplete, divergent or not yielded by exporter",
        ));
    }
    Ok(())
}

fn repository(root: &Path) -> Result<(PathBuf, String), DecapodError> {
    if DbBroker::new(root).is_cloud() {
        return Err(fail(
            "transfer supports the local canonical task schema only; no cloud fallback",
        ));
    }
    let repo = repo_root_from_store_root(root)?;
    let name = repo_identity::resolve_repository_identity(&repo)?
        .canonical_name
        .to_ascii_lowercase();
    Ok((repo, name))
}

fn export_bundle(root: &Path, id: &str, to: &str, actor: &str) -> Result<Bundle, DecapodError> {
    if !actor_valid(actor) || !actor_valid(to) || actor == to {
        return Err(fail(
            "distinct explicit source and receiver agent identities are required",
        ));
    }
    let (repo, name) = repository(root)?;
    let source_commit = super::super::run_git(&repo, &["rev-parse", "HEAD"])?;
    let source_branch = super::super::run_git(&repo, &["symbolic-ref", "--short", "HEAD"])?;
    let broker = DbBroker::new(root);
    with_task_journal(
        &broker,
        &todo_db_path(root),
        actor,
        None,
        "todo.transfer.export",
        TaskJournalSelection::Ids(vec![id.into()]),
        |conn| {
            // No active lease is handed off: yield already relinquished it. Keep
            // ordinary exclusive-claim trust; receiver acceptance is explicit.
            enforce_operation_policy(root, conn, "todo.claim.exclusive", actor, id)?;
            let ids = vec![id.to_owned()];
            let current = capture_selected(conn, &ids)?;
            let projection = current
                .get(id)
                .ok_or_else(|| fail("source task does not exist"))?
                .clone();
            standalone(&projection)?;
            let incoming = capture(conn)?.values().any(|p| {
                p.task.id != id
                    && (p.task.parent_task_id.as_deref() == Some(id)
                        || p.dependencies.iter().any(|d| d.depends_on_task_id == id)
                        || [p.task.depends_on.as_deref(), p.task.blocks.as_deref()]
                            .into_iter()
                            .flatten()
                            .any(|s| {
                                super::super::parse_dependency_ids(s)
                                    .iter()
                                    .any(|d| d == id)
                            }))
            });
            if incoming {
                return Err(fail(
                    "task has incoming relationships; transfer closure is unsupported",
                ));
            }
            let journal = read_journal(conn, Some(&ids))?;
            let through = watermark(conn)?;
            check_complete_suffix(conn, &journal, through)?;
            let head = journal.heads.get(id).ok_or_else(|| {
                fail("source journal is missing; inspect and checkpoint current source state first")
            })?;
            if head.projection != projection {
                return Err(fail("source projection differs from journal"));
            }
            let history =
                events::query_subject_on_conn(conn, events::TODO, "task", id, MAX_HISTORY)?
                    .into_iter()
                    .map(HistoricalEvent::from)
                    .collect();
            let bundle = Bundle {
                schema: SCHEMA.into(),
                transfer_id: crate::core::ulid::new_ulid(),
                repository: name.clone(),
                source_commit: source_commit.trim().into(),
                source_branch: source_branch.trim().into(),
                exporter: actor.into(),
                receiver: to.into(),
                exported_at: now_iso(),
                source_watermark: through,
                source_head: head.record_digest.clone(),
                sealed_revision: projection
                    .task
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| fail("revision overflow"))?,
                projection,
                history,
            };
            validate(&bundle)?;
            let bytes = encode(&bundle)?;
            conn.execute(
                "UPDATE tasks SET revision=?1, updated_at=?2 WHERE id=?3",
                params![bundle.sealed_revision, bundle.exported_at, id],
            )?;
            events::append_on_conn(
                conn,
                events::TODO,
                &json!({
                    "event_id":format!("task-transfer-export:{}",bundle.transfer_id),
                    "ts":bundle.exported_at,"event_type":EXPORTED,"status":"success","task_id":id,"actor":actor,
                    "payload":{"bundle":bundle,"artifact_sha256":sha(&bytes),"source_sealed":true}
                }),
            )?;
            Ok(bundle)
        },
    )
}

fn recover_bundle(
    root: &Path,
    id: &str,
    transfer_id: &str,
    actor: &str,
) -> Result<Bundle, DecapodError> {
    let (_, name) = repository(root)?;
    let conn = crate::core::db::db_connect_for_validate(
        todo_db_path(root)
            .to_str()
            .ok_or_else(|| fail("invalid datastore path"))?,
    )?;
    // The immutable export is addressed directly: exporting appends its own
    // event and checkpoint, and comments may grow the subject after sealing.
    // Neither may consume the recovery reader's history budget.
    let event = events::query_subject_event_on_conn(
        &conn,
        events::TODO,
        "task",
        id,
        &format!("task-transfer-export:{transfer_id}"),
    )?
    .ok_or_else(|| fail("no matching durable export"))?;
    let bundle: Bundle = serde_json::from_value(event.payload["payload"]["bundle"].clone())
        .map_err(|_| fail("malformed durable export"))?;
    validate(&bundle)?;
    if bundle.repository != name
        || bundle.projection.task.id != id
        || bundle.transfer_id != transfer_id
        || bundle.exporter != actor
        || event.event_type != EXPORTED
        || event.actor != actor
        || event.payload["event_id"] != event.event_id
        || event.payload["event_type"] != EXPORTED
        || event.payload["task_id"] != id
        || event.payload["actor"] != actor
        || event.payload["ts"] != event.ts
        || event.payload["payload"]["source_sealed"] != true
        || event.payload["payload"]["artifact_sha256"] != sha(&encode(&bundle)?)
    {
        return Err(fail("durable export binding mismatch"));
    }
    Ok(bundle)
}

fn import_bundle(root: &Path, bundle: &Bundle, actor: &str) -> Result<Value, DecapodError> {
    validate(bundle)?;
    let (repo, name) = repository(root)?;
    if bundle.repository != name || bundle.receiver != actor {
        return Err(fail("wrong repository or receiver"));
    }
    if super::super::run_git(&repo, &["cat-file", "-t", &bundle.source_commit])?.trim() != "commit"
    {
        return Err(fail("fetch the preserved source commit before import"));
    }
    let id = &bundle.projection.task.id;
    let broker = DbBroker::new(root);
    with_task_journal(
        &broker,
        &todo_db_path(root),
        actor,
        None,
        "todo.transfer.import",
        TaskJournalSelection::Ids(vec![id.clone()]),
        |conn| {
            enforce_operation_policy(root, conn, "todo.claim.exclusive", actor, id)?;
            // Any earlier local record for this subject is conflicting authority,
            // even if somebody removed its current projection. Never merge histories.
            let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 OR hash=?2) OR EXISTS(SELECT 1 FROM events WHERE stream='todo' AND (subject_id=?1 OR event_id=?3))",
            params![id,bundle.projection.task.hash,format!("task-transfer-import:{}",bundle.transfer_id)], |r| r.get(0))?;
            if exists {
                return Err(fail(
                    "existing task/hash/history or replayed transfer; no state replaced",
                ));
            }
            let mut local = bundle.projection.clone();
            local.task.revision = bundle
                .sealed_revision
                .checked_add(1)
                .ok_or_else(|| fail("revision overflow"))?;
            local.task.dir_path = repo.to_string_lossy().into_owned();
            local.task.updated_at = now_iso();
            // Retain source proof bytes only in the imported provenance below.
            // A new revision and empty current proof prevent cross-machine reuse.
            if let Some(v) = &mut local.verification {
                v.verification_artifacts = None;
                v.last_verified_at = None;
                v.last_verified_status = Some("unavailable".into());
                v.last_verified_notes =
                    Some("Imported historical proof requires regeneration on this machine".into());
                v.updated_at = local.task.updated_at.clone();
            }
            validate_projection(&local)?;
            write_task(conn, &local.task)?;
            write_children(conn, &local)?;
            events::append_on_conn(
                conn,
                events::TODO,
                &json!({
                    "event_id":format!("task-transfer-import:{}",bundle.transfer_id),
                    "ts":now_iso(),"event_type":IMPORTED,"status":"success","task_id":id,"actor":actor,
                    "payload":{"bundle":bundle,"artifact_sha256":sha(&encode(bundle)?),
                        "authority":"operator-reviewed source transfer; local cooperative enforcement",
                        "proof_policy":"historical_only_regenerate_locally"}
                }),
            )?;
            Ok(
                json!({"status":"ok","task_id":id,"transfer_id":bundle.transfer_id,
            "revision":local.task.revision,"execution_ready":false,
            "next":"claim the imported task, ensure its workspace, orient and regenerate proof"}),
            )
        },
    )
}

fn output_parent(path: &Path) -> Result<&Path, DecapodError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err(fail(
            "artifact output must be an absolute path without traversal",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| fail("artifact parent missing"))?;
    fs_permissions::ensure_private_dir(parent)?;
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(fail("artifact destination already exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(parent)
}
fn publish(path: &Path, bundle: &Bundle) -> Result<Value, DecapodError> {
    let parent = output_parent(path)?;
    let bytes = encode(bundle)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".task-transfer-")
        .tempfile_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|e| DecapodError::IoError(e.error))?;
    atomic::sync_parent_directory(parent)?;
    Ok(json!({"status":"ok","task_id":bundle.projection.task.id,
        "transfer_id":bundle.transfer_id,"artifact":path,"artifact_sha256":sha(&bytes),
        "source_sealed":true,"receiver":bundle.receiver,
        "trust":"digest binds bytes; operator must authenticate source handoff"}))
}

pub(in super::super) fn run(
    root: &Path,
    command: &TodoTransferCommand,
    actor: &str,
) -> Result<Value, DecapodError> {
    match command {
        TodoTransferCommand::Export { id, to, output } => {
            output_parent(output)?; // Fail before sealing for a known bad destination.
            let bundle = export_bundle(root, id, to, actor)?;
            publish(output, &bundle).map_err(|e| fail(&format!(
                "source sealed as {}; artifact publication unconfirmed: {e}. Use transfer recover --id {id} --transfer-id {} --output <new-private-path>",
                bundle.transfer_id, bundle.transfer_id)))
        }
        TodoTransferCommand::Recover {
            id,
            transfer_id,
            output,
        } => publish(output, &recover_bundle(root, id, transfer_id, actor)?),
        TodoTransferCommand::Import {
            input,
            expected_digest,
            accept_source_transfer,
        } => {
            if !accept_source_transfer {
                return Err(fail("explicit operator source acceptance required"));
            }
            if !input.is_absolute() {
                return Err(fail("artifact input must be absolute"));
            }
            repository(root)?; // Never fall back from cloud routing.
            let file = fs_permissions::open_private_file(input, OpenOptions::new().read(true))?;
            if file.metadata()?.len() > MAX_PACKAGE as u64 {
                return Err(fail("artifact exceeds byte bound"));
            }
            let mut bytes = Vec::new();
            file.take(MAX_PACKAGE as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > MAX_PACKAGE || sha(&bytes) != *expected_digest {
                return Err(fail(
                    "artifact byte bound or independently supplied digest mismatch",
                ));
            }
            let bundle: Bundle =
                serde_json::from_slice(&bytes).map_err(|_| fail("malformed artifact"))?;
            import_bundle(root, &bundle, actor)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/task_transfer_tests.rs"]
mod tests;
