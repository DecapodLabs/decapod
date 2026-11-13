//! Versioned task projection checkpoints in the canonical TODO event stream.
//!
//! Legacy domain events are evidence, not a complete replay source. The first
//! checkpoint explicitly observes existing state; only subsequent complete
//! checkpoints form a reconstructible chain. Proof strings are preserved exactly.
//!
//! Schema v2 externalizes large immutable string components (`>= 128` UTF-8 bytes)
//! into content-addressed `storage_components` + `storage_component_refs` rows inside
//! the same SQLite transaction while preserving exact `TaskProjection` reconstruction
//! and `projection_digest` parity across v1 and v2 records.

use crate::core::{
    db::{Connection, OptionalExtension, params},
    error::DecapodError,
    events, schemas,
    storage_codec::{self, StoredText},
};
use dactyl_db::Parameter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[path = "task_transfer.rs"]
pub(super) mod transfer;

pub(super) const EVENT_TYPE: &str = "task.journal.checkpoint";
/// Reviewed acceptance of a historical uncheckpointed suffix (#1534). Exempt
/// from the complete-suffix check like other journal bookkeeping events.
pub(super) const EVENT_TYPE_GAP_ACCEPTED: &str = "task.journal.gap_accepted";
const MIN_SCHEMA_VERSION: u32 = 1;
const SCHEMA_VERSION: u32 = 2;
pub(crate) const COMPONENT_EXTERNALIZE_MIN_BYTES: usize = 128;
pub(crate) const COMPONENT_REF_SCOPE_CHECKPOINT: &str = "task_checkpoint";
const MAX_TASKS: usize = 10_000;
const MAX_RECORDS: usize = 100_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_VERIFY_PAGE_SIZE: usize = 256;

const COMP_TASK_DESCRIPTION: &str = "task.description";
const COMP_TASK_SCOPE: &str = "task.scope";
const COMP_VERIFICATION_PROOF_PLAN: &str = "verification.proof_plan";
const COMP_VERIFICATION_ARTIFACTS: &str = "verification.verification_artifacts";
const COMP_VERIFICATION_NOTES: &str = "verification.last_verified_notes";

fn invalid(message: impl AsRef<str>) -> DecapodError {
    DecapodError::ValidationError(format!("TASK_JOURNAL_CONFLICT: {}", message.as_ref()))
}
fn digest<T: Serialize>(value: &T) -> Result<String, DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require_transaction(conn: &Connection) -> Result<(), DecapodError> {
    if conn.is_autocommit() {
        return Err(invalid("mutation requires the caller's transaction"));
    }
    Ok(())
}

struct HashJsonWriter<'a>(&'a mut Sha256);
impl std::io::Write for HashJsonWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(crate) struct ComponentRef {
    pub kind: String,
    pub codec_version: u32,
    pub byte_len: u64,
    pub content_sha256: String,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ts: Option<String>,
    projection: TaskProjection,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    component_refs: BTreeMap<String, ComponentRef>,
    projection_digest: String,
    record_digest: String,
}

fn clear_externalized_projection_fields(
    projection: &mut TaskProjection,
    component_refs: &BTreeMap<String, ComponentRef>,
) -> Result<(), DecapodError> {
    for field_key in component_refs.keys() {
        match field_key.as_str() {
            COMP_TASK_DESCRIPTION => {
                if projection.task.description.is_none() {
                    return Err(invalid("component_ref targets absent task.description"));
                }
                projection.task.description = Some(String::new());
            }
            COMP_TASK_SCOPE => {
                projection.task.scope.clear();
            }
            COMP_VERIFICATION_PROOF_PLAN => {
                let v = projection.verification.as_mut().ok_or_else(|| {
                    invalid("component_ref targets absent verification.proof_plan")
                })?;
                v.proof_plan.clear();
            }
            COMP_VERIFICATION_ARTIFACTS => {
                let v = projection.verification.as_mut().ok_or_else(|| {
                    invalid("component_ref targets absent verification.verification_artifacts")
                })?;
                if v.verification_artifacts.is_none() {
                    return Err(invalid(
                        "component_ref targets null verification.verification_artifacts",
                    ));
                }
                v.verification_artifacts = Some(String::new());
            }
            COMP_VERIFICATION_NOTES => {
                let v = projection.verification.as_mut().ok_or_else(|| {
                    invalid("component_ref targets absent verification.last_verified_notes")
                })?;
                if v.last_verified_notes.is_none() {
                    return Err(invalid(
                        "component_ref targets null verification.last_verified_notes",
                    ));
                }
                v.last_verified_notes = Some(String::new());
            }
            other => {
                return Err(invalid(format!(
                    "unsupported checkpoint component_ref key '{other}'"
                )));
            }
        }
    }
    Ok(())
}

impl Checkpoint {
    fn hash(&self) -> Result<String, DecapodError> {
        let mut value = self.clone();
        value.record_digest.clear();
        if !value.component_refs.is_empty() {
            clear_externalized_projection_fields(&mut value.projection, &value.component_refs)?;
        }
        digest(&value)
    }
}

struct PreparedExternalComponent {
    cref: ComponentRef,
    stored_bytes: Vec<u8>,
}

type ExternalizedProjection = (
    TaskProjection,
    BTreeMap<String, ComponentRef>,
    Vec<PreparedExternalComponent>,
);

fn prepare_externalized_projection(
    projection: &TaskProjection,
) -> Result<ExternalizedProjection, DecapodError> {
    let mut wire = projection.clone();
    let mut refs = BTreeMap::new();
    let mut components = Vec::new();

    let mut maybe_externalize = |field_key: &str, text: &str| -> Result<bool, DecapodError> {
        if text.len() < COMPONENT_EXTERNALIZE_MIN_BYTES {
            return Ok(false);
        }
        let encoded = storage_codec::encode(text)?;
        let (codec_version, stored_bytes) = match encoded {
            StoredText::Zlib(blob) => (1u32, blob),
            StoredText::Plain(plain) => (0u32, plain.into_bytes()),
        };
        let cref = ComponentRef {
            kind: field_key.to_owned(),
            codec_version,
            byte_len: text.len() as u64,
            content_sha256: sha256_hex(text.as_bytes()),
        };
        refs.insert(field_key.to_owned(), cref.clone());
        components.push(PreparedExternalComponent { cref, stored_bytes });
        Ok(true)
    };

    if let Some(desc) = projection.task.description.as_deref()
        && maybe_externalize(COMP_TASK_DESCRIPTION, desc)?
    {
        wire.task.description = Some(String::new());
    }
    if maybe_externalize(COMP_TASK_SCOPE, &projection.task.scope)? {
        wire.task.scope.clear();
    }
    if let Some(ver) = projection.verification.as_ref() {
        if maybe_externalize(COMP_VERIFICATION_PROOF_PLAN, &ver.proof_plan)?
            && let Some(wv) = wire.verification.as_mut()
        {
            wv.proof_plan.clear();
        }
        if let Some(art) = ver.verification_artifacts.as_deref()
            && maybe_externalize(COMP_VERIFICATION_ARTIFACTS, art)?
            && let Some(wv) = wire.verification.as_mut()
        {
            wv.verification_artifacts = Some(String::new());
        }
        if let Some(notes) = ver.last_verified_notes.as_deref()
            && maybe_externalize(COMP_VERIFICATION_NOTES, notes)?
            && let Some(wv) = wire.verification.as_mut()
        {
            wv.last_verified_notes = Some(String::new());
        }
    }

    Ok((wire, refs, components))
}

pub(crate) fn decode_and_verify_component_bytes(
    cref: &ComponentRef,
    raw_bytes: &[u8],
) -> Result<String, DecapodError> {
    let decoded = match cref.codec_version {
        0 => std::str::from_utf8(raw_bytes)
            .map_err(|_| invalid("externalized plain component is not valid UTF-8"))?
            .to_owned(),
        1 => storage_codec::decode(&Parameter::Blob(raw_bytes.to_vec()))
            .map_err(|e| invalid(format!("corrupt zlib externalized component: {e}")))?,
        other => {
            return Err(invalid(format!(
                "unsupported externalized component codec_version {other}"
            )));
        }
    };
    if decoded.len() as u64 != cref.byte_len
        || sha256_hex(decoded.as_bytes()) != cref.content_sha256
    {
        return Err(invalid(
            "externalized component byte length or SHA-256 digest mismatch",
        ));
    }
    Ok(decoded)
}

pub(crate) fn load_component_text_on_conn(
    conn: &Connection,
    cref: &ComponentRef,
) -> Result<String, DecapodError> {
    if !conn.has_table("storage_components")? {
        return Err(invalid(
            "checkpoint references storage_components, but table is absent",
        ));
    }
    let raw: Option<Vec<u8>> = conn
        .query_row(
            "SELECT stored_bytes FROM storage_components
             WHERE component_kind = ?1
               AND codec_version = ?2
               AND byte_len = ?3
               AND content_sha256 = ?4",
            params![
                cref.kind,
                cref.codec_version,
                cref.byte_len,
                cref.content_sha256
            ],
            |r| r.get(0),
        )
        .optional()?;
    let raw_bytes = raw.ok_or_else(|| {
        invalid(format!(
            "missing externalized component ({}, sha256={})",
            cref.kind, cref.content_sha256
        ))
    })?;
    decode_and_verify_component_bytes(cref, &raw_bytes)
}

pub(crate) fn persist_component_utf8_on_conn(
    conn: &Connection,
    cref: &ComponentRef,
    utf8_value: &str,
    ref_scope: &str,
    ref_id: &str,
    created_at: &str,
) -> Result<(), DecapodError> {
    if utf8_value.len() as u64 != cref.byte_len
        || sha256_hex(utf8_value.as_bytes()) != cref.content_sha256
    {
        return Err(invalid(
            "transferred component UTF-8 content does not match declared digest",
        ));
    }
    let stored_bytes = match cref.codec_version {
        0 => utf8_value.as_bytes().to_vec(),
        1 => match storage_codec::encode(utf8_value)? {
            StoredText::Zlib(blob) => blob,
            StoredText::Plain(_) => {
                return Err(invalid(
                    "transferred component declared zlib codec_version=1 for incompressible payload",
                ));
            }
        },
        other => {
            return Err(invalid(format!(
                "unsupported transferred component codec_version {other}"
            )));
        }
    };
    conn.execute_batch(schemas::STORAGE_COMPONENTS_SCHEMA)?;
    conn.execute_batch(schemas::STORAGE_COMPONENT_REFS_SCHEMA)?;
    conn.execute(
        "INSERT OR IGNORE INTO storage_components(
             component_kind, codec_version, byte_len, content_sha256, stored_bytes, created_at
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            cref.kind,
            cref.codec_version,
            cref.byte_len,
            cref.content_sha256,
            stored_bytes,
            created_at
        ],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO storage_component_refs(
             ref_scope, ref_id, component_kind, codec_version, byte_len, content_sha256
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            ref_scope,
            ref_id,
            cref.kind,
            cref.codec_version,
            cref.byte_len,
            cref.content_sha256
        ],
    )?;
    Ok(())
}

fn hydrate_checkpoint_with<F>(
    checkpoint: &mut Checkpoint,
    mut resolve: F,
) -> Result<(), DecapodError>
where
    F: FnMut(&ComponentRef) -> Result<String, DecapodError>,
{
    if checkpoint.schema_version == 1 && !checkpoint.component_refs.is_empty() {
        return Err(invalid(
            "schema_version 1 checkpoint cannot carry v2 component_refs",
        ));
    }
    for (field_key, cref) in &checkpoint.component_refs {
        if cref.kind != *field_key {
            return Err(invalid("checkpoint component_ref key/kind mismatch"));
        }
        let check_or_resolve = |inline: &str, cref: &ComponentRef, resolve: &mut F| {
            if inline.is_empty() {
                resolve(cref)
            } else if inline.len() as u64 == cref.byte_len
                && sha256_hex(inline.as_bytes()) == cref.content_sha256
            {
                Ok(inline.to_owned())
            } else {
                Err(invalid(
                    "non-empty wire projection field disagrees with component_ref",
                ))
            }
        };
        match field_key.as_str() {
            COMP_TASK_DESCRIPTION => {
                let cur = checkpoint
                    .projection
                    .task
                    .description
                    .as_deref()
                    .ok_or_else(|| invalid("component_ref targets null task.description"))?;
                let hydrated = check_or_resolve(cur, cref, &mut resolve)?;
                checkpoint.projection.task.description = Some(hydrated);
            }
            COMP_TASK_SCOPE => {
                let hydrated =
                    check_or_resolve(&checkpoint.projection.task.scope, cref, &mut resolve)?;
                checkpoint.projection.task.scope = hydrated;
            }
            COMP_VERIFICATION_PROOF_PLAN => {
                let v = checkpoint
                    .projection
                    .verification
                    .as_mut()
                    .ok_or_else(|| invalid("component_ref targets missing verification"))?;
                v.proof_plan = check_or_resolve(&v.proof_plan, cref, &mut resolve)?;
            }
            COMP_VERIFICATION_ARTIFACTS => {
                let v = checkpoint
                    .projection
                    .verification
                    .as_mut()
                    .ok_or_else(|| invalid("component_ref targets missing verification"))?;
                let cur = v
                    .verification_artifacts
                    .as_deref()
                    .ok_or_else(|| invalid("component_ref targets null verification_artifacts"))?;
                v.verification_artifacts = Some(check_or_resolve(cur, cref, &mut resolve)?);
            }
            COMP_VERIFICATION_NOTES => {
                let v = checkpoint
                    .projection
                    .verification
                    .as_mut()
                    .ok_or_else(|| invalid("component_ref targets missing verification"))?;
                let cur = v
                    .last_verified_notes
                    .as_deref()
                    .ok_or_else(|| invalid("component_ref targets null last_verified_notes"))?;
                v.last_verified_notes = Some(check_or_resolve(cur, cref, &mut resolve)?);
            }
            other => {
                return Err(invalid(format!(
                    "unsupported checkpoint component_ref key '{other}'"
                )));
            }
        }
    }
    Ok(())
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

fn check_no_orphan_projection_rows(conn: &Connection) -> Result<(), DecapodError> {
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
    check_no_orphan_projection_rows(conn)?;
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
            let reconstructed = events::reconstruct_stored_payload(
                events::TODO,
                &event_id,
                &ts,
                &event_type,
                &actor,
                subject_kind.as_deref(),
                subject_id.as_deref(),
                &payload,
            )
            .map_err(|e| invalid(format!("corrupt checkpoint JSON: {e}")))?;
            out.push(CheckpointEvent {
                event: events::StoredEvent {
                    stream: events::TODO.into(),
                    event_id,
                    ts,
                    seq,
                    event_type,
                    payload: reconstructed,
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
    journal_from_records_with_resolver(checkpoint_events(conn, ids)?, |cref| {
        load_component_text_on_conn(conn, cref)
    })
}

fn validate_and_hydrate_record<F>(
    record: &CheckpointEvent,
    resolve: &mut F,
) -> Result<Checkpoint, DecapodError>
where
    F: FnMut(&ComponentRef) -> Result<String, DecapodError>,
{
    let event = &record.event;
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
    let mut checkpoint: Checkpoint = serde_json::from_value(
        envelope
            .get("payload")
            .cloned()
            .ok_or_else(|| invalid("missing checkpoint payload"))?,
    )
    .map_err(|e| invalid(format!("invalid checkpoint: {e}")))?;

    if checkpoint.schema_version < MIN_SCHEMA_VERSION || checkpoint.schema_version > SCHEMA_VERSION
    {
        return Err(invalid(
            "checkpoint identity/schema/digest/watermark mismatch",
        ));
    }
    if checkpoint
        .ts
        .as_deref()
        .is_some_and(|recorded_ts| recorded_ts != event.ts.as_str())
    {
        return Err(invalid("checkpoint event envelope mismatch"));
    }
    if checkpoint.record_digest != checkpoint.hash()?
        || event.event_id != format!("task-journal:{}", checkpoint.record_digest)
    {
        return Err(invalid(
            "checkpoint identity/schema/digest/watermark mismatch",
        ));
    }
    hydrate_checkpoint_with(&mut checkpoint, resolve)?;
    validate_projection(&checkpoint.projection)?;
    if checkpoint.task_id != checkpoint.projection.task.id
        || envelope.get("task_id").and_then(|v| v.as_str()) != Some(checkpoint.task_id.as_str())
        || checkpoint.actor != event.actor
        || checkpoint.projection_digest != digest(&checkpoint.projection)?
        || checkpoint.source_watermark >= event.seq
    {
        return Err(invalid(
            "checkpoint identity/schema/digest/watermark mismatch",
        ));
    }
    Ok(checkpoint)
}

fn journal_from_records_with_resolver<F>(
    records: Vec<CheckpointEvent>,
    mut resolve: F,
) -> Result<Journal, DecapodError>
where
    F: FnMut(&ComponentRef) -> Result<String, DecapodError>,
{
    let mut journal = Journal::default();
    for record in records {
        if record.event.event_type != EVENT_TYPE {
            continue;
        }
        let checkpoint = validate_and_hydrate_record(&record, &mut resolve)?;
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

#[allow(dead_code)]
fn journal_from_records(records: Vec<CheckpointEvent>) -> Result<Journal, DecapodError> {
    journal_from_records_with_resolver(records, |_| {
        Err(invalid(
            "checkpoint references externalized component without a component store resolver",
        ))
    })
}

/// A surviving domain mutation after the last complete snapshot means the tail
/// is incomplete. Never replay an older snapshot over that evidence. Unknown
/// subject events fail closed; only known read-only/audit events are exempt.
fn check_complete_suffix(
    conn: &Connection,
    journal: &Journal,
    through: u64,
    skip: Option<&BTreeSet<String>>,
) -> Result<(), DecapodError> {
    for (id, head) in &journal.heads {
        if skip.is_some_and(|ids| ids.contains(id)) {
            continue;
        }
        check_complete_suffix_for_task(conn, id, head.source_watermark, through)?;
    }
    Ok(())
}

fn check_complete_suffix_for_task(
    conn: &Connection,
    id: &str,
    source_watermark: u64,
    through: u64,
) -> Result<(), DecapodError> {
    let incomplete: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND subject_id=?1 AND seq>?2 AND seq<=?3 AND event_type NOT IN ('task.journal.checkpoint','task.journal.restored','task.journal.gap_accepted','task.comment','task.worker.run','task.verify.capture_failed'))",
        params![id, source_watermark, through],
        |r| r.get(0),
    )?;
    if incomplete {
        return Err(invalid(format!(
            "TASK_JOURNAL_INCOMPLETE_HISTORY: uncheckpointed domain-event suffix for {id}"
        )));
    }
    Ok(())
}

/// Execution meaning is unchanged by releasing custody or recording completion.
/// Every omitted field is still checked through the allowed journal transitions.
fn validation_contract_digest(projection: &TaskProjection) -> Result<String, DecapodError> {
    let mut contract = projection.clone();
    contract.task.status.clear();
    contract.task.updated_at.clear();
    contract.task.completed_at = None;
    contract.task.closed_at = None;
    contract.task.owner = None;
    contract.task.assigned_to = None;
    contract.task.assigned_at = None;
    contract.task.lease_expires_at = None;
    contract.task.lease_lifecycle = None;
    contract.task.revision = 0;
    contract.owners.clear();
    // Bind declared task obligations while excluding changing execution results.
    // The two gate names express the same full-validation requirement at
    // different lifecycle stages; neither discharges any additional task gate.
    let mut gates: Vec<String> = match &projection.verification {
        Some(verification) => serde_json::from_str(&verification.proof_plan).map_err(|_| {
            invalid("VALIDATION_HISTORY_TASK_STALE: malformed declared task proof plan")
        })?,
        None => vec!["validate_passes".into()],
    };
    if gates.is_empty() || gates.iter().any(|gate| gate.trim().is_empty()) {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: empty declared task proof plan",
        ));
    }
    for gate in &mut gates {
        if gate == "validated_commit" {
            *gate = "validate_passes".into();
        }
    }
    gates.sort();
    gates.dedup();
    contract.verification = Some(VerificationRow {
        proof_plan: serde_json::to_string(&gates).map_err(|e| invalid(e.to_string()))?,
        verification_artifacts: None,
        last_verified_at: None,
        last_verified_status: None,
        last_verified_notes: None,
        verification_policy_days: 0,
        updated_at: String::new(),
    });
    digest(&contract)
}

pub(super) fn capture_validation_binding(
    conn: &Connection,
    task_id: &str,
) -> Result<super::HistoricalTaskBinding, DecapodError> {
    let ids = vec![task_id.to_owned()];
    let journal = read_journal(conn, Some(&ids))?;
    check_complete_suffix(conn, &journal, watermark(conn)?, None)?;
    let head = journal.heads.get(task_id).ok_or_else(|| {
        invalid("VALIDATION_HISTORY_TASK_UNBOUND: no complete canonical task checkpoint")
    })?;
    let current = capture_selected(conn, &ids)?;
    if current.get(task_id) != Some(&head.projection) {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_UNBOUND: live task differs from its journal",
        ));
    }
    let task = &head.projection.task;
    let assignee = task.assigned_to.clone().unwrap_or_default();
    if assignee.is_empty()
        || assignee != crate::current_agent_id()
        || !matches!(task.status.as_str(), "open" | "in_progress" | "done")
        || task.lease_generation.unwrap_or(0) <= 0
        || !super::LeaseLifecycle::parse(task.lease_lifecycle.as_deref()).holds_exclusive_custody()
        || super::fleet_coord::lease_state(task.lease_expires_at.as_deref(), &super::now_iso())
            != super::fleet_coord::LeaseState::Active
    {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_UNBOUND: validation requires current exclusive task custody",
        ));
    }
    Ok(super::HistoricalTaskBinding {
        task_id: task_id.to_owned(),
        revision: task.revision,
        status: task.status.clone(),
        contract_digest: validation_contract_digest(&head.projection)?,
        checkpoint_digest: head.record_digest.clone(),
        assignee,
        lease_generation: task.lease_generation.unwrap_or(0),
    })
}

/// A release/completion chronology may advance revision, but changed meaning,
/// reassignment, restored history, or a new lease can never reuse old proof.
pub(super) fn check_validation_binding(
    conn: &Connection,
    binding: &super::HistoricalTaskBinding,
    allow_lifecycle_progress: bool,
) -> Result<(), DecapodError> {
    let ids = vec![binding.task_id.clone()];
    let journal = read_journal(conn, Some(&ids))?;
    check_complete_suffix(conn, &journal, watermark(conn)?, None)?;
    let head = journal
        .heads
        .get(&binding.task_id)
        .ok_or_else(|| invalid("VALIDATION_HISTORY_TASK_STALE: missing canonical task history"))?;
    let current = capture_selected(conn, &ids)?;
    if current.get(&binding.task_id) != Some(&head.projection) {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: live task differs from its journal",
        ));
    }
    if !allow_lifecycle_progress && head.record_digest != binding.checkpoint_digest {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: task changed during validation",
        ));
    }
    if !allow_lifecycle_progress {
        // Time can revoke live validation custody without changing a journal
        // row. Historical consumption intentionally does not require a live
        // lease, but the producer must still hold it when appending success.
        let task = &head.projection.task;
        if task.assigned_to.as_deref() != Some(binding.assignee.as_str())
            || !super::LeaseLifecycle::parse(task.lease_lifecycle.as_deref())
                .holds_exclusive_custody()
            || super::fleet_coord::lease_state(task.lease_expires_at.as_deref(), &super::now_iso())
                != super::fleet_coord::LeaseState::Active
        {
            return Err(invalid(
                "VALIDATION_HISTORY_TASK_STALE: live validation custody expired or was released before archival",
            ));
        }
    }
    let checkpoints = checkpoint_events(conn, Some(&ids))?;
    let mut previous: Option<Checkpoint> = None;
    let mut captured_watermark = None;
    for record in checkpoints {
        let mut checkpoint: Checkpoint =
            serde_json::from_value(record.event.payload["payload"].clone())
                .map_err(|error| invalid(error.to_string()))?;
        hydrate_checkpoint_with(&mut checkpoint, |cref| {
            load_component_text_on_conn(conn, cref)
        })?;
        if let Some(prior) = previous.as_ref() {
            if !allow_lifecycle_progress {
                return Err(invalid(
                    "VALIDATION_HISTORY_TASK_STALE: task advanced during validation",
                ));
            }
            check_validation_lifecycle_step(binding, prior, &checkpoint)?;
        } else {
            if checkpoint.record_digest != binding.checkpoint_digest {
                continue;
            }
            let task = &checkpoint.projection.task;
            if task.id != binding.task_id
                || task.revision != binding.revision
                || task.status != binding.status
                || task.assigned_to.as_deref() != Some(binding.assignee.as_str())
                || binding.assignee.is_empty()
                || task.lease_generation != Some(binding.lease_generation)
                || validation_contract_digest(&checkpoint.projection)? != binding.contract_digest
            {
                return Err(invalid(
                    "VALIDATION_HISTORY_TASK_STALE: captured task identity differs",
                ));
            }
            captured_watermark = Some(checkpoint.source_watermark);
        }
        previous = Some(checkpoint);
    }
    if previous.as_ref().map(|value| &value.record_digest) != Some(&head.record_digest) {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: captured checkpoint is absent",
        ));
    }
    let altered_history: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND (subject_id=?1 OR subject_id IS NULL) AND seq>?2 AND event_type IN ('task.journal.restored','task.journal.gap_accepted'))",
        params![binding.task_id, captured_watermark.unwrap_or(0)],
        |row| row.get(0),
    )?;
    if altered_history {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: restored or incomplete chronology requires new proof",
        ));
    }
    Ok(())
}

fn check_validation_lifecycle_step(
    binding: &super::HistoricalTaskBinding,
    previous: &Checkpoint,
    current: &Checkpoint,
) -> Result<(), DecapodError> {
    let proof_only = matches!(
        current.operation.as_str(),
        "todo.proof.claimed" | "verify.history.complete" | "verify.persist"
    );
    let lifecycle = matches!(
        current.operation.as_str(),
        "todo.yield" | "todo.release" | "todo.session.cleanup" | "task.done"
    );
    let task = &current.projection.task;
    let prior = &previous.projection.task;
    let assigned = task.assigned_to.as_deref().unwrap_or("");
    let same_status = task.status == prior.status;
    let completing = current.operation == "task.done"
        && matches!(prior.status.as_str(), "open" | "in_progress")
        && task.status == "done";
    let expected_revision = if proof_only {
        prior.revision
    } else {
        prior
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("task revision overflow"))?
    };
    if (!proof_only && !lifecycle)
        || task.revision != expected_revision
        || (!same_status && !completing)
        || !matches!(task.status.as_str(), "open" | "in_progress" | "done")
        || (!assigned.is_empty() && assigned != binding.assignee)
        || task.lease_generation != Some(binding.lease_generation)
        || validation_contract_digest(&current.projection)? != binding.contract_digest
        || (proof_only && task != prior)
    {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: task intent, dependency, custody or chronology changed",
        ));
    }
    // Once released, an old record cannot renew authoring authority even under
    // the same actor name. A normal new claim changes generation and is rejected.
    if prior.assigned_to.as_deref().unwrap_or("").is_empty() && !assigned.is_empty() {
        return Err(invalid(
            "VALIDATION_HISTORY_TASK_STALE: released custody was reacquired",
        ));
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
    let event_ts = super::now_iso();
    let projection_digest = digest(projection)?;
    let (wire_projection, component_refs, external_components) =
        prepare_externalized_projection(projection)?;

    let mut wire_checkpoint = Checkpoint {
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
        ts: Some(event_ts.clone()),
        projection: wire_projection,
        component_refs,
        projection_digest,
        record_digest: String::new(),
    };
    wire_checkpoint.record_digest = wire_checkpoint.hash()?;
    let event_id = format!("task-journal:{}", wire_checkpoint.record_digest);

    if !external_components.is_empty() {
        conn.execute_batch(schemas::STORAGE_COMPONENTS_SCHEMA)?;
        conn.execute_batch(schemas::STORAGE_COMPONENT_REFS_SCHEMA)?;
        for item in &external_components {
            conn.execute(
                "INSERT OR IGNORE INTO storage_components(
                     component_kind, codec_version, byte_len, content_sha256, stored_bytes, created_at
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    item.cref.kind,
                    item.cref.codec_version,
                    item.cref.byte_len,
                    item.cref.content_sha256,
                    item.stored_bytes,
                    event_ts
                ],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO storage_component_refs(
                     ref_scope, ref_id, component_kind, codec_version, byte_len, content_sha256
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    COMPONENT_REF_SCOPE_CHECKPOINT,
                    event_id,
                    item.cref.kind,
                    item.cref.codec_version,
                    item.cref.byte_len,
                    item.cref.content_sha256
                ],
            )?;
        }
    }

    let envelope = serde_json::json!({
        "ts": event_ts,
        "event_id": event_id,
        "event_type": EVENT_TYPE,
        "status": "success",
        "task_id": wire_checkpoint.task_id,
        "actor": actor,
        "payload": wire_checkpoint,
    });
    let added_bytes = serde_json::to_vec(&envelope)
        .map_err(|e| invalid(e.to_string()))?
        .len();
    let (count, bytes): (i64, i64) = conn.query_row("SELECT count(*), COALESCE(sum(length(CAST(payload AS BLOB))+length(CAST(event_id AS BLOB))+length(CAST(ts AS BLOB))+length(CAST(actor AS BLOB))),0) FROM events WHERE stream='todo' AND event_type=?1 AND subject_id=?2", params![EVENT_TYPE,wire_checkpoint.task_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
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

    let mut hydrated = wire_checkpoint;
    hydrated.projection = projection.clone();
    journal
        .history
        .entry(hydrated.task_id.clone())
        .or_default()
        .insert(hydrated.projection_digest.clone());
    journal.heads.insert(hydrated.task_id.clone(), hydrated);
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
    check_complete_suffix(conn, &journal, before_watermark, None)?;
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
                "SELECT EXISTS(SELECT 1 FROM events WHERE stream='todo' AND subject_id=?1 AND seq>?2 AND event_type NOT IN ('task.journal.checkpoint','task.journal.restored','task.journal.gap_accepted','task.comment','task.worker.run','task.verify.capture_failed'))",
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
        "complete": true,
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
    check_complete_suffix(conn, &journal, watermark(conn)?, None)?;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyBudget {
    pub page_size: usize,
    pub max_checkpoint_rows: Option<usize>,
    pub max_task_rows: Option<usize>,
}

impl VerifyBudget {
    pub fn unbounded() -> Self {
        Self {
            page_size: DEFAULT_VERIFY_PAGE_SIZE,
            max_checkpoint_rows: None,
            max_task_rows: None,
        }
    }
}

impl Default for VerifyBudget {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskHeadSummary {
    ordinal: u64,
    record_digest: String,
    projection_digest: String,
    source_watermark: u64,
    revision: i64,
    lease_generation: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifyPhase {
    Checkpoints,
    Tasks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyCursor {
    pub phase: VerifyPhase,
    pub last_seq: u64,
    pub last_event_id: String,
    pub last_task_id: String,
    pub snapshot_watermark: u64,
    pub snapshot_task_count: u64,
    pub snapshot_checkpoint_count: u64,
    pub snapshot_revision_sum: i64,
    pub records_verified: usize,
    heads: BTreeMap<String, TaskHeadSummary>,
    divergent_tasks: Vec<String>,
    uncheckpointed_tasks: Vec<String>,
    seen_live_tasks: BTreeSet<String>,
}

fn read_snapshot_fingerprint(conn: &Connection) -> Result<(u64, u64, u64, i64), DecapodError> {
    let wm = watermark(conn)?;
    let (task_count, rev_sum): (u64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(revision), 0) FROM tasks",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let ckpt_count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM events WHERE stream='todo' AND event_type=?1",
        [EVENT_TYPE],
        |r| r.get(0),
    )?;
    Ok((wm, task_count, ckpt_count, rev_sum))
}

fn compute_streaming_current_digest(
    conn: &Connection,
    page_size: usize,
) -> Result<String, DecapodError> {
    let mut hasher = Sha256::new();
    hasher.update(b"{");
    let mut first = true;
    let mut last_id = String::new();
    let step = page_size.clamp(1, MAX_TASKS);
    loop {
        let mut stmt =
            conn.prepare("SELECT id FROM tasks WHERE id > ?1 ORDER BY id ASC LIMIT ?2")?;
        let rows = stmt.query_map(params![last_id, step as i64], |r| r.get::<_, String>(0))?;
        let ids = rows.collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            break;
        }
        last_id = ids.last().expect("non-empty page").clone();
        let page = capture_selected(conn, &ids)?;
        for id in &ids {
            let Some(proj) = page.get(id) else {
                return Err(invalid(
                    "TASK_JOURNAL_SNAPSHOT_CHANGED: live task vanished during snapshot digest",
                ));
            };
            if !first {
                hasher.update(b",");
            }
            first = false;
            serde_json::to_writer(&mut HashJsonWriter(&mut hasher), id)
                .map_err(|e| invalid(e.to_string()))?;
            hasher.update(b":");
            serde_json::to_writer(&mut HashJsonWriter(&mut hasher), proj)
                .map_err(|e| invalid(e.to_string()))?;
        }
    }
    hasher.update(b"}");
    Ok(format!("{:x}", hasher.finalize()))
}

/// Snapshot-consistent keyset-paged task journal verification without global
/// 10k-task / 100k-record / 32 MiB hard ceilings. Supports optional caller row
/// budgets returning truthful `"status": "partial"` + `continuation_cursor`.
pub(crate) fn verify_with_budget(
    conn: &Connection,
    budget: &VerifyBudget,
    cursor: Option<&VerifyCursor>,
) -> Result<serde_json::Value, DecapodError> {
    let page_size = budget.page_size.clamp(1, 2048);
    let started_snapshot = if !conn.is_read_only() && conn.is_autocommit() {
        conn.execute_batch("BEGIN DEFERRED")?;
        true
    } else {
        false
    };

    let result = (|| -> Result<serde_json::Value, DecapodError> {
        check_schema(conn)?;
        check_no_orphan_projection_rows(conn)?;

        let (wm, task_count, ckpt_count, rev_sum) = read_snapshot_fingerprint(conn)?;
        let mut state = match cursor {
            Some(prior) => {
                if (
                    prior.snapshot_watermark,
                    prior.snapshot_task_count,
                    prior.snapshot_checkpoint_count,
                    prior.snapshot_revision_sum,
                ) != (wm, task_count, ckpt_count, rev_sum)
                {
                    return Err(invalid(
                        "TASK_JOURNAL_SNAPSHOT_CHANGED: datastore mutated between paged verification cursors; restart verification on a fresh snapshot",
                    ));
                }
                prior.clone()
            }
            None => VerifyCursor {
                phase: VerifyPhase::Checkpoints,
                last_seq: 0,
                last_event_id: String::new(),
                last_task_id: String::new(),
                snapshot_watermark: wm,
                snapshot_task_count: task_count,
                snapshot_checkpoint_count: ckpt_count,
                snapshot_revision_sum: rev_sum,
                records_verified: 0,
                heads: BTreeMap::new(),
                divergent_tasks: Vec::new(),
                uncheckpointed_tasks: Vec::new(),
                seen_live_tasks: BTreeSet::new(),
            },
        };

        if state.phase == VerifyPhase::Checkpoints {
            let mut processed_in_call = 0usize;
            loop {
                let remaining = budget
                    .max_checkpoint_rows
                    .map(|max| max.saturating_sub(processed_in_call));
                if remaining == Some(0) {
                    let has_more: bool = conn.query_row(
                        "SELECT EXISTS(
                             SELECT 1 FROM events
                             WHERE stream='todo' AND event_type=?1
                               AND (seq > ?2 OR (seq = ?2 AND event_id > ?3))
                         )",
                        params![EVENT_TYPE, state.last_seq, state.last_event_id],
                        |r| r.get(0),
                    )?;
                    if has_more {
                        return Ok(serde_json::json!({
                            "schema_version": "decapod.task-journal.v1",
                            "status": "partial",
                            "complete": false,
                            "limitation": "caller_budget_reached",
                            "journal_valid": true,
                            "records_verified": state.records_verified,
                            "tasks_verified": state.seen_live_tasks.len(),
                            "checkpointed_tasks": state.heads.len(),
                            "continuation_cursor": state,
                        }));
                    }
                    state.phase = VerifyPhase::Tasks;
                    break;
                }

                let fetch_limit = remaining
                    .map(|r| r.min(page_size))
                    .unwrap_or(page_size)
                    .max(1);

                let largest_in_page: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(len), 0) FROM (
                         SELECT length(CAST(payload AS BLOB)) AS len
                         FROM events
                         WHERE stream='todo' AND event_type=?1
                           AND (seq > ?2 OR (seq = ?2 AND event_id > ?3))
                         ORDER BY seq ASC, event_id ASC
                         LIMIT ?4
                     )",
                    params![
                        EVENT_TYPE,
                        state.last_seq,
                        state.last_event_id,
                        fetch_limit as i64
                    ],
                    |r| r.get(0),
                )?;
                if largest_in_page > MAX_RECORD_BYTES as i64 {
                    return Err(invalid(
                        "journal record exceeds bounded replay limit; explicit checkpoint migration required",
                    ));
                }

                let mut stmt = conn.prepare(
                    "SELECT event_id, ts, seq, event_type, payload, actor, subject_id, subject_kind
                     FROM events
                     WHERE stream='todo' AND event_type=?1
                       AND (seq > ?2 OR (seq = ?2 AND event_id > ?3))
                     ORDER BY seq ASC, event_id ASC
                     LIMIT ?4",
                )?;
                let rows = stmt.query_map(
                    params![
                        EVENT_TYPE,
                        state.last_seq,
                        state.last_event_id,
                        fetch_limit as i64
                    ],
                    |r| {
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
                    },
                )?;
                let page = rows.collect::<Result<Vec<_>, _>>()?;
                if page.is_empty() {
                    state.phase = VerifyPhase::Tasks;
                    break;
                }

                for (event_id, ts, seq, event_type, raw_payload, actor, subject_id, subject_kind) in
                    page
                {
                    let reconstructed = events::reconstruct_stored_payload(
                        events::TODO,
                        &event_id,
                        &ts,
                        &event_type,
                        &actor,
                        subject_kind.as_deref(),
                        subject_id.as_deref(),
                        &raw_payload,
                    )
                    .map_err(|e| invalid(format!("corrupt checkpoint JSON: {e}")))?;

                    let record = CheckpointEvent {
                        event: events::StoredEvent {
                            stream: events::TODO.into(),
                            event_id: event_id.clone(),
                            ts,
                            seq,
                            event_type,
                            payload: reconstructed,
                            actor,
                        },
                        subject_id,
                        subject_kind,
                    };
                    let checkpoint = validate_and_hydrate_record(&record, &mut |cref| {
                        load_component_text_on_conn(conn, cref)
                    })?;

                    match state.heads.get(&checkpoint.task_id) {
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
                                || checkpoint.predecessor.as_deref()
                                    != Some(previous.record_digest.as_str())
                                || checkpoint.previous_digest.as_deref()
                                    != Some(previous.projection_digest.as_str())
                                || checkpoint.projection.task.revision < previous.revision
                                || checkpoint.projection.task.lease_generation.unwrap_or(0)
                                    < previous.lease_generation
                                || checkpoint.source_watermark < previous.source_watermark
                            {
                                return Err(invalid(format!(
                                    "contradictory, truncated or regressing chain for {}",
                                    checkpoint.task_id
                                )));
                            }
                        }
                    }

                    state.heads.insert(
                        checkpoint.task_id.clone(),
                        TaskHeadSummary {
                            ordinal: checkpoint.ordinal,
                            record_digest: checkpoint.record_digest,
                            projection_digest: checkpoint.projection_digest,
                            source_watermark: checkpoint.source_watermark,
                            revision: checkpoint.projection.task.revision,
                            lease_generation: checkpoint
                                .projection
                                .task
                                .lease_generation
                                .unwrap_or(0),
                        },
                    );
                    state.last_seq = seq;
                    state.last_event_id = event_id;
                    state.records_verified += 1;
                    processed_in_call += 1;
                }
            }
        }

        // Complete-suffix verification over all discovered head summaries in a single
        // O(E) keyset-paged pass over idx_events_stream_seq_unique (avoiding O(T * E) scans).
        if !state.heads.is_empty() {
            let min_watermark = state
                .heads
                .values()
                .map(|h| h.source_watermark)
                .min()
                .unwrap_or(0);
            let mut suffix_seq = min_watermark;
            loop {
                let mut stmt = conn.prepare(
                    "SELECT seq, subject_id
                     FROM events
                     WHERE stream = 'todo'
                       AND seq > ?1
                       AND seq <= ?2
                       AND subject_id IS NOT NULL
                       AND event_type NOT IN (
                           'task.journal.checkpoint',
                           'task.journal.restored',
                           'task.journal.gap_accepted',
                           'task.comment',
                           'task.worker.run',
                           'task.verify.capture_failed'
                       )
                     ORDER BY seq ASC
                     LIMIT 512",
                )?;
                let rows = stmt.query_map(params![suffix_seq, wm], |r| {
                    Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?))
                })?;
                let batch = rows.collect::<Result<Vec<_>, _>>()?;
                if batch.is_empty() {
                    break;
                }
                suffix_seq = batch.last().expect("non-empty suffix batch").0;
                for (seq, subject_id) in batch {
                    if let Some(head) = state.heads.get(&subject_id)
                        && seq > head.source_watermark
                    {
                        return Err(invalid(format!(
                            "TASK_JOURNAL_INCOMPLETE_HISTORY: uncheckpointed domain-event suffix for {subject_id}"
                        )));
                    }
                }
            }
        }

        // Keyset-paged live task verification.
        let mut tasks_processed_in_call = 0usize;
        loop {
            let remaining = budget
                .max_task_rows
                .map(|max| max.saturating_sub(tasks_processed_in_call));
            if remaining == Some(0) {
                let has_more: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE id > ?1)",
                    [&state.last_task_id],
                    |r| r.get(0),
                )?;
                if has_more {
                    return Ok(serde_json::json!({
                        "schema_version": "decapod.task-journal.v1",
                        "status": "partial",
                        "complete": false,
                        "limitation": "caller_budget_reached",
                        "journal_valid": true,
                        "records_verified": state.records_verified,
                        "tasks_verified": state.seen_live_tasks.len(),
                        "checkpointed_tasks": state.heads.len(),
                        "continuation_cursor": state,
                    }));
                }
                break;
            }

            let fetch_limit = remaining
                .map(|r| r.min(page_size).min(MAX_TASKS))
                .unwrap_or_else(|| page_size.min(MAX_TASKS))
                .max(1);
            let mut stmt =
                conn.prepare("SELECT id FROM tasks WHERE id > ?1 ORDER BY id ASC LIMIT ?2")?;
            let rows = stmt.query_map(params![state.last_task_id, fetch_limit as i64], |r| {
                r.get::<_, String>(0)
            })?;
            let ids = rows.collect::<Result<Vec<_>, _>>()?;
            if ids.is_empty() {
                break;
            }
            let page = capture_selected(conn, &ids)?;
            for id in ids {
                let Some(live_proj) = page.get(&id) else {
                    return Err(invalid(
                        "TASK_JOURNAL_SNAPSHOT_CHANGED: live task disappeared mid-page",
                    ));
                };
                match state.heads.get(&id) {
                    None => state.uncheckpointed_tasks.push(id.clone()),
                    Some(head) => {
                        if digest(live_proj)? != head.projection_digest {
                            state.divergent_tasks.push(id.clone());
                        }
                    }
                }
                state.seen_live_tasks.insert(id.clone());
                state.last_task_id = id;
                tasks_processed_in_call += 1;
            }
        }

        let missing_tasks: Vec<String> = state
            .heads
            .keys()
            .filter(|id| !state.seen_live_tasks.contains(*id))
            .cloned()
            .collect();
        let current_digest = compute_streaming_current_digest(conn, page_size)?;
        if read_snapshot_fingerprint(conn)? != (wm, task_count, ckpt_count, rev_sum) {
            return Err(invalid(
                "TASK_JOURNAL_SNAPSHOT_CHANGED: datastore mutated during verification scan; restart verification",
            ));
        }
        let status = if state.divergent_tasks.is_empty()
            && missing_tasks.is_empty()
            && state.uncheckpointed_tasks.is_empty()
        {
            "ok"
        } else {
            "conflict"
        };

        Ok(serde_json::json!({
            "schema_version": "decapod.task-journal.v1",
            "status": status,
            "complete": true,
            "journal_valid": true,
            "records": state.records_verified,
            "checkpointed_tasks": state.heads.len(),
            "current_digest": current_digest,
            "divergent_tasks": state.divergent_tasks,
            "missing_tasks": missing_tasks,
            "uncheckpointed_tasks": state.uncheckpointed_tasks,
            "history_origin": "observed_current_state_baselines",
            "proof_policy": "preserve_bytes_no_refresh",
        }))
    })();

    if started_snapshot {
        let _ = conn.execute_batch("ROLLBACK");
    }
    result
}

pub(super) fn verify(conn: &Connection) -> Result<serde_json::Value, DecapodError> {
    verify_with_budget(conn, &VerifyBudget::unbounded(), None)
}

#[cfg(test)]
pub(super) fn checkpoint(
    conn: &Connection,
    actor: &str,
) -> Result<serde_json::Value, DecapodError> {
    checkpoint_with_gap(conn, actor, None)
}

/// Explicit, reviewed repair for historical uncheckpointed domain-event
/// suffixes (#1534, #1581). Each accepted task gets its own durable evidence
/// range and transition checkpoint. All preconditions and non-exempt heads are
/// checked before the first write; the caller owns the enclosing transaction.
#[cfg(test)]
pub(super) fn checkpoint_with_gap(
    conn: &Connection,
    actor: &str,
    gap: Option<(&str, &str)>,
) -> Result<serde_json::Value, DecapodError> {
    let gaps: Vec<(String, String)> = gap
        .map(|(id, reason)| vec![(id.to_owned(), reason.to_owned())])
        .unwrap_or_default();
    checkpoint_with_gaps(conn, actor, &gaps)
}

pub(super) fn checkpoint_with_gaps(
    conn: &Connection,
    actor: &str,
    gaps: &[(String, String)],
) -> Result<serde_json::Value, DecapodError> {
    require_transaction(conn)?;
    if gaps.len() > MAX_TASKS {
        return Err(invalid(
            "historical-gap selection exceeds bounded task limit",
        ));
    }
    let current = capture(conn)?;
    let source_watermark = watermark(conn)?;
    let mut journal = read_journal(conn, None)?;
    let mut gap_task_ids = BTreeSet::new();
    let mut reviewed = Vec::with_capacity(gaps.len());

    // Preflight every task, its suffix, and its live projection before writing
    // any acceptance event. A duplicate ID or unrelated divergence aborts all.
    for (id, reason) in gaps {
        if id.trim().is_empty() || reason.trim().is_empty() || reason.len() > 2048 {
            return Err(invalid(
                "historical-gap task ID and bounded reviewed reason are required",
            ));
        }
        if !gap_task_ids.insert(id.clone()) {
            return Err(invalid(format!("duplicate historical-gap task {id}")));
        }
        let head = journal.heads.get(id).ok_or_else(|| {
            invalid(format!(
                "no checkpointed history for {id}; --accept-historical-gap only repairs journaled tasks"
            ))
        })?;
        let (count, from_seq, through_seq): (i64, Option<u64>, Option<u64>) = conn.query_row(
            "SELECT count(*), min(seq), max(seq) FROM events WHERE stream='todo' AND subject_id=?1 AND seq>?2 AND seq<=?3 AND event_type NOT IN ('task.journal.checkpoint','task.journal.restored','task.journal.gap_accepted','task.comment','task.worker.run','task.verify.capture_failed')",
            params![id, head.source_watermark, source_watermark],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if count == 0 {
            return Err(invalid(format!(
                "no uncheckpointed domain-event suffix to accept for {id}"
            )));
        }
        let projection = current.get(id).cloned().ok_or_else(|| {
            invalid("baseline cannot replace divergent or missing journaled state")
        })?;
        reviewed.push((
            id.clone(),
            reason.clone(),
            count,
            from_seq.unwrap_or(0),
            through_seq.unwrap_or(0),
            projection,
        ));
    }

    check_complete_suffix(conn, &journal, source_watermark, Some(&gap_task_ids))?;
    for (id, head) in &journal.heads {
        let diverged = current.get(id).map(digest).transpose()?.as_deref()
            != Some(head.projection_digest.as_str());
        if diverged && !gap_task_ids.contains(id) {
            return Err(invalid(
                "baseline cannot replace divergent or missing journaled state",
            ));
        }
    }

    let mut accepted = Vec::with_capacity(reviewed.len());
    for (id, reason, count, from_seq, through_seq, projection) in &reviewed {
        events::append_on_conn(
            conn,
            events::TODO,
            &serde_json::json!({
                "ts": super::now_iso(),
                "event_id": format!("task-journal-gap:{id}:{}:{through_seq}", super::now_iso()),
                "event_type": EVENT_TYPE_GAP_ACCEPTED,
                "status": "success",
                "task_id": id,
                "actor": actor,
                "payload": {
                    "task_id": id,
                    "reason": reason,
                    "accepted_from_seq": from_seq,
                    "accepted_through_seq": through_seq,
                    "accepted_event_count": count,
                    "repair": "explicit_reviewed_acceptance; historical events are preserved, not replayed"
                }
            }),
        )?;
        append_checkpoint(
            conn,
            &mut journal,
            projection,
            source_watermark,
            "historical_gap_accepted",
            actor,
        )?;
        accepted.push(serde_json::json!({
            "task_id": id,
            "reason": reason,
            "accepted_from_seq": from_seq,
            "accepted_through_seq": through_seq,
            "accepted_event_count": count,
        }));
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
    let mut result = report(conn, &journal)?;
    if !accepted.is_empty() {
        if accepted.len() == 1 {
            result["accepted_historical_gap"] = accepted[0].clone();
        }
        result["accepted_historical_gaps"] = serde_json::Value::Array(accepted);
    }
    Ok(result)
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
    check_complete_suffix(conn, &journal, watermark(conn)?, None)?;
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

#[cfg(test)]
#[path = "../../../tests/unit/core/task_journal_historical_tests.rs"]
mod historical_tests;
