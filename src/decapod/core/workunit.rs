use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::context_capsule::DeterministicContextCapsule;
use crate::core::error;
use crate::core::validation_epoch::ValidationEpochMetadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkUnitStatus {
    Draft,
    Executing,
    Claimed,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkUnitProofResult {
    pub gate: String,
    pub status: String,
    pub artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_epoch: Option<ValidationEpochMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkUnitManifest {
    pub task_id: String,
    pub intent_ref: String,
    pub spec_refs: Vec<String>,
    pub state_refs: Vec<String>,
    pub proof_plan: Vec<String>,
    pub proof_results: Vec<WorkUnitProofResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_epoch: Option<ValidationEpochMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_record: Option<WorkUnitValidationReference>,
    pub status: WorkUnitStatus,
}

/// A selector and integrity binding into the existing canonical history.
/// These public fields are references, never producer authentication by themselves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkUnitValidationReference {
    pub candidate_sha: String,
    pub record_id: String,
    pub record_digest: String,
}

/// Original observed subject: immutable history, not a new proof authority.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkUnitValidationSubject {
    pub contract_digest: String,
    pub manifest_digest: String,
    pub generation: u64,
}

pub(crate) fn validation_contract_digest(
    manifest: &WorkUnitManifest,
) -> Result<String, error::DecapodError> {
    let canonical = manifest.canonicalized();
    crate::core::readiness_evidence::digest(&(
        "decapod.workunit-contract.v1",
        &canonical.task_id,
        &canonical.intent_ref,
        &canonical.spec_refs,
        &canonical.state_refs,
        &canonical.proof_plan,
    ))
    .map_err(|e| journal_error("WORKUNIT_PROOF_UNVERIFIABLE", e))
}

/// Observe the journal before/after validation, preserving undeclared versus
/// deleted, divergent or unbound state. No caller-supplied success is consumed.
pub(crate) fn capture_validation_subject(
    root: &Path,
    task_id: &str,
) -> Result<Option<WorkUnitValidationSubject>, error::DecapodError> {
    let records = journal_records(root, task_id)?;
    let Some(record) = records.last() else {
        for path in [
            workunit_path(root, task_id)?,
            root.join(".decapod/governance/workunits")
                .join(format!("{task_id}.json")),
        ] {
            match fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    return Err(journal_error(
                        "WORKUNIT_CHECKPOINT_REQUIRED",
                        "legacy or unknown projection requires an explicit current checkpoint",
                    ));
                }
            }
        }
        return Ok(None);
    };
    if !projection_matches(root, record)? {
        return Err(journal_error(
            "WORKUNIT_RECOVERY_REQUIRED",
            "workunit projection differs from its journal",
        ));
    }
    verify_resume_record(root, record)?;
    Ok(Some(WorkUnitValidationSubject {
        contract_digest: validation_contract_digest(&record.manifest)?,
        manifest_digest: record.manifest_hash.clone(),
        generation: record.generation,
    }))
}

fn manifest_validation_reference(
    manifest: &WorkUnitManifest,
) -> Result<Option<(&str, &str)>, error::DecapodError> {
    let Some(reference) = &manifest.validation_record else {
        return Ok(None);
    };
    if reference.record_id.is_empty()
        || reference.record_id.len() > 128
        || !reference
            .record_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "malformed canonical validation reference",
        ));
    }
    Ok(Some((&reference.candidate_sha, &reference.record_id)))
}

impl WorkUnitManifest {
    pub fn canonicalized(&self) -> Self {
        let mut out = self.clone();

        out.spec_refs.sort();
        out.spec_refs.dedup();

        out.state_refs.sort();
        out.state_refs.dedup();

        out.proof_plan.sort();
        out.proof_plan.dedup();

        out.proof_results.sort();

        out
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn canonical_hash_hex(&self) -> Result<String, serde_json::Error> {
        let bytes = self.canonical_json_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

pub fn workunits_dir(project_root: &Path) -> PathBuf {
    project_root.join(".decapod").join("data").join("workunits")
}

pub fn validate_task_id(task_id: &str) -> Result<(), error::DecapodError> {
    if task_id.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "task_id cannot be empty".to_string(),
        ));
    }
    if task_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(error::DecapodError::ValidationError(format!(
            "invalid task_id '{task_id}': allowed characters are [A-Za-z0-9_-]"
        )))
    }
}

/// Read-only preflight: ignored workunit state moves to data/, never into Git governance.
pub(crate) fn validate_legacy_workunits(project_root: &Path) -> Result<(), error::DecapodError> {
    let legacy = project_root.join(".decapod/governance/workunits");
    let mut ancestor = project_root.to_path_buf();
    for component in [".decapod", "governance", "workunits"] {
        ancestor.push(component);
        if fs::symlink_metadata(&ancestor).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(error::DecapodError::ValidationError(
                "Symlink legacy workunit path is not migratable".into(),
            ));
        }
    }
    if !legacy.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(&legacy)
        .map_err(error::DecapodError::IoError)?
        .file_type()
        .is_symlink()
    {
        return Err(error::DecapodError::ValidationError(
            "Symlink legacy workunit directory is not migratable".into(),
        ));
    }
    for entry in fs::read_dir(&legacy).map_err(error::DecapodError::IoError)? {
        let path = entry.map_err(error::DecapodError::IoError)?.path();
        let metadata = fs::symlink_metadata(&path).map_err(error::DecapodError::IoError)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || path.extension().and_then(|s| s.to_str()) != Some("json")
        {
            return Err(error::DecapodError::ValidationError(
                "Unexpected legacy workunit file; explicit recovery required".into(),
            ));
        }
        let bytes = read_manifest_bytes(&path)?;
        let manifest: WorkUnitManifest = serde_json::from_slice(&bytes).map_err(|e| {
            error::DecapodError::ValidationError(format!("Invalid legacy workunit: {e}"))
        })?;
        validate_task_id(&manifest.task_id)?;
        if path.file_stem().and_then(|s| s.to_str()) != Some(&manifest.task_id) {
            return Err(error::DecapodError::ValidationError(
                "Workunit filename/identity mismatch".into(),
            ));
        }
        let destination = workunit_path(project_root, &manifest.task_id)?;
        if destination.exists() && read_manifest_bytes(&destination)? != bytes {
            return Err(error::DecapodError::ValidationError(
                "Conflicting legacy/runtime workunit state; no migration performed".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn migrate_legacy_workunits(project_root: &Path) -> Result<(), error::DecapodError> {
    validate_legacy_workunits(project_root)?;
    let legacy = project_root.join(".decapod/governance/workunits");
    if !legacy.exists() {
        return Ok(());
    }
    crate::core::fs_permissions::ensure_storage_dir(&workunits_dir(project_root))
        .map_err(error::DecapodError::IoError)?;
    for entry in fs::read_dir(&legacy).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let destination = workunits_dir(project_root).join(entry.file_name());
        if destination.exists() {
            if read_manifest_bytes(&entry.path())? != read_manifest_bytes(&destination)? {
                return Err(error::DecapodError::ValidationError(
                    "Workunit changed during migration".into(),
                ));
            }
            fs::remove_file(entry.path()).map_err(error::DecapodError::IoError)?;
        } else {
            fs::rename(entry.path(), destination).map_err(error::DecapodError::IoError)?;
        }
    }
    fs::remove_dir(legacy).map_err(error::DecapodError::IoError)
}

pub fn workunit_path(project_root: &Path, task_id: &str) -> Result<PathBuf, error::DecapodError> {
    validate_task_id(task_id)?;
    Ok(workunits_dir(project_root).join(format!("{task_id}.json")))
}

// Runtime journal records deliberately remain outside the Git-owned governance document.
// The manifest is a projection; a durable event is committed before its file is published.
const JOURNAL_SCHEMA: &str = "decapod.workunit-journal.v1";
const JOURNAL_EVENT: &str = "workunit.snapshot";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WorkUnitJournalRecord {
    schema_version: String,
    event_type: String,
    event_id: String,
    ts: String,
    actor: String,
    task_id: String,
    operation_id: String,
    kind: String,
    generation: u64,
    predecessor: Option<String>,
    manifest_hash: String,
    manifest: WorkUnitManifest,
    binding: Option<crate::core::trajectory::ExecutionStateBinding>,
    proof_hashes: std::collections::BTreeMap<String, String>,
    legacy_baseline: bool,
    record_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkUnitJournalStatus {
    pub task_id: String,
    pub generation: Option<u64>,
    pub checkpoint_id: Option<String>,
    pub checkpoint_hash: Option<String>,
    pub projection_state: String,
    pub legacy_baseline: bool,
    /// Resume permission is never inferred from successful file recovery.
    pub resumable: bool,
    pub resume_blocker: Option<String>,
}

fn journal_error(code: &str, detail: impl std::fmt::Display) -> error::DecapodError {
    error::DecapodError::ValidationError(format!("{code}: {detail}"))
}

fn journal_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn record_hash(record: &WorkUnitJournalRecord) -> Result<String, error::DecapodError> {
    let mut record = record.clone();
    record.record_hash.clear();
    serde_json::to_vec(&record)
        .map(|bytes| journal_hash(&bytes))
        .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))
}

fn operation_event_id(task_id: &str, operation_id: &str) -> String {
    journal_hash(format!("{JOURNAL_SCHEMA}\0{task_id}\0{operation_id}").as_bytes())
}

fn validate_operation_id(operation_id: &str) -> Result<(), error::DecapodError> {
    if operation_id.trim().is_empty()
        || operation_id.len() > 256
        || operation_id.chars().any(char::is_control)
    {
        return Err(journal_error(
            "WORKUNIT_OPERATION_INVALID",
            "use a nonempty bounded operation ID",
        ));
    }
    Ok(())
}

/// Resolve the owning `.decapod/data` directory for workunit runtime policy
/// checks and journal persistence.
///
/// Linked Git worktrees (`<worktree>/.git` file) share the initialized
/// control-plane datastore (`agents`, `risk_zones`, `events`) at
/// `<main_repo>/.decapod/data`, unless a complete worktree-local datastore
/// with policy tables was explicitly initialized at `<worktree>/.decapod/data`.
fn runtime_data_dir(root: &Path) -> PathBuf {
    let local = root.join(".decapod/data");
    if root.join(".git").is_file() {
        let local_initialized = {
            let db = crate::core::events::canonical_db_path(&local);
            db.is_file()
                && crate::core::db::db_connect_for_validate(&db.to_string_lossy())
                    .ok()
                    .is_some_and(|conn| {
                        conn.has_table("agents").unwrap_or(false)
                            && conn.has_table("risk_zones").unwrap_or(false)
                    })
        };
        if !local_initialized
            && let Ok(main_root) = crate::core::workspace::get_main_repo_root(root)
            && main_root != root
        {
            return main_root.join(".decapod/data");
        }
    }
    local
}

/// Workunit mutations require the existing initialized runtime policy owner.
/// Never create an events-only datastore that cannot authorize its next write,
/// or silently fill missing policy tables with a new default policy.
fn require_initialized_workunit_runtime(root: &Path) -> Result<(), error::DecapodError> {
    let path = crate::core::events::canonical_db_path(&runtime_data_dir(root));
    let uninitialized = || {
        journal_error(
            "WORKUNIT_RUNTIME_UNINITIALIZED",
            "initialize the repository with `decapod init` before workunit mutation; an existing partial datastore requires explicit initialization/reconciliation of its agents and risk_zones policy tables",
        )
    };
    if !path.exists() {
        return Err(uninitialized());
    }
    let conn = crate::core::db::db_connect_for_validate(&path.to_string_lossy())?;
    if !conn.has_table("agents")? || !conn.has_table("risk_zones")? {
        return Err(uninitialized());
    }
    Ok(())
}

// Unlike the branch-local governance lock, this lock is on the shared runtime
// volume. Containers/worktrees exposing that volume cannot race file publication.
fn with_workunit_lock<T>(
    root: &Path,
    f: impl FnOnce() -> Result<T, error::DecapodError>,
) -> Result<T, error::DecapodError> {
    require_initialized_workunit_runtime(root)?;
    use fs2::FileExt;
    struct Lease(std::fs::File);
    impl Drop for Lease {
        fn drop(&mut self) {
            let _ = FileExt::unlock(&self.0);
        }
    }
    crate::core::governance_document::with_lock(root, || {
        let dir = workunits_dir(root);
        crate::core::fs_permissions::ensure_storage_dir(&dir)?;
        let file = crate::core::fs_permissions::open_storage_file(
            &dir.join(".journal.lock"),
            fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false),
        )?;
        let start = std::time::Instant::now();
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < std::time::Duration::from_secs(5) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => return Err(journal_error("WORKUNIT_JOURNAL_BUSY", e)),
            }
        }
        let _lease = Lease(file);
        migrate_legacy_workunits(root)?;
        f()
    })
}

fn journal_records_on_conn(
    conn: &crate::core::db::Connection,
    task_id: &str,
) -> Result<Vec<WorkUnitJournalRecord>, error::DecapodError> {
    let stored = crate::core::events::query_subject_on_conn(
        conn,
        crate::core::events::WORKUNIT,
        "task",
        task_id,
        10_000,
    )?; // ascending complete history; overflow fails rather than truncating
    let mut records: Vec<WorkUnitJournalRecord> = Vec::new();
    let mut operation_ids = std::collections::BTreeSet::new();
    for event in stored {
        let record: WorkUnitJournalRecord = serde_json::from_value(event.payload)
            .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?;
        if record.schema_version != JOURNAL_SCHEMA
            || record.event_type != JOURNAL_EVENT
            || event.event_type != JOURNAL_EVENT
            || record.event_id != event.event_id
            || record.actor != event.actor
            || record.ts != event.ts
            || record.event_id != operation_event_id(&record.task_id, &record.operation_id)
            || record.manifest.task_id != record.task_id
            || record.manifest_hash
                != record
                    .manifest
                    .canonical_hash_hex()
                    .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?
            || record.record_hash != record_hash(&record)?
            || !matches!(
                record.kind.as_str(),
                "initialized" | "legacy_baseline" | "mutation" | "checkpoint" | "resumed"
            )
        {
            return Err(journal_error(
                "WORKUNIT_JOURNAL_INVALID",
                "unsupported schema or mismatched event/snapshot identity",
            ));
        }
        if record.task_id != task_id {
            return Err(journal_error(
                "WORKUNIT_JOURNAL_INVALID",
                "indexed task subject differs from payload identity",
            ));
        }
        let previous = records.last();
        let generation = previous.map_or(1, |prior| prior.generation.saturating_add(1));
        if record.generation != generation
            || record.predecessor.as_deref() != previous.map(|prior| prior.record_hash.as_str())
            || (previous.is_none()
                && (!matches!(record.kind.as_str(), "initialized" | "legacy_baseline")
                    || record.legacy_baseline != (record.kind == "legacy_baseline")))
            || (previous.is_some()
                && matches!(record.kind.as_str(), "initialized" | "legacy_baseline"))
            || previous.is_some_and(|prior| prior.legacy_baseline != record.legacy_baseline)
            || !operation_ids.insert(record.operation_id.clone())
        {
            return Err(journal_error(
                "WORKUNIT_JOURNAL_CONFLICT",
                "missing, reordered, duplicate, or contradictory predecessor history",
            ));
        }
        records.push(record);
    }
    Ok(records)
}

fn journal_records(
    root: &Path,
    task_id: &str,
) -> Result<Vec<WorkUnitJournalRecord>, error::DecapodError> {
    validate_task_id(task_id)?;
    let path = crate::core::events::canonical_db_path(&runtime_data_dir(root));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = crate::core::db::db_connect_for_validate(&path.to_string_lossy())?;
    journal_records_on_conn(&conn, task_id)
}

const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

fn read_manifest_bytes(path: &Path) -> Result<Vec<u8>, error::DecapodError> {
    use std::io::Read;
    crate::core::fs_permissions::check_file(path, true)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_MANIFEST_BYTES {
        return Err(journal_error(
            "WORKUNIT_PROJECTION_INVALID",
            "manifest exceeds regular-file byte bound",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(journal_error(
            "WORKUNIT_PROJECTION_INVALID",
            "manifest grew beyond its byte bound",
        ));
    }
    Ok(bytes)
}

fn load_workunit_projection(
    root: &Path,
    task_id: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let mut path = workunit_path(root, task_id)?;
    let legacy = root
        .join(".decapod/governance/workunits")
        .join(format!("{task_id}.json"));
    if !path.exists() && legacy.exists() {
        path = legacy;
    }
    if !path.exists() {
        return Err(error::DecapodError::NotFound(format!(
            "workunit '{task_id}' not found at {}",
            path.display()
        )));
    }
    crate::core::fs_permissions::check_file(&path, true)?;
    let raw = read_manifest_bytes(&path)?;
    let manifest: WorkUnitManifest = serde_json::from_slice(&raw)
        .map_err(|e| journal_error("WORKUNIT_PROJECTION_INVALID", e))?;
    if manifest.task_id != task_id {
        return Err(journal_error(
            "WORKUNIT_PROJECTION_INVALID",
            "filename/task identity differs",
        ));
    }
    Ok(manifest)
}

fn projection_matches(
    root: &Path,
    record: &WorkUnitJournalRecord,
) -> Result<bool, error::DecapodError> {
    Ok(load_workunit_projection(root, &record.task_id)?
        .canonical_hash_hex()
        .map_err(|e| journal_error("WORKUNIT_PROJECTION_INVALID", e))?
        == record.manifest_hash)
}

fn publish_projection(
    root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<PathBuf, error::DecapodError> {
    use std::io::Write;
    let path = workunit_path(root, &manifest.task_id)?;
    let parent = path
        .parent()
        .ok_or_else(|| journal_error("WORKUNIT_PROJECTION_INVALID", "missing parent"))?;
    crate::core::fs_permissions::ensure_storage_dir(parent)?;
    crate::core::fs_permissions::check_file(&path, true)?;
    let bytes = serde_json::to_vec_pretty(&manifest.canonicalized())
        .map_err(|e| journal_error("WORKUNIT_PROJECTION_INVALID", e))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(journal_error(
            "WORKUNIT_PROJECTION_INVALID",
            "manifest exceeds byte bound",
        ));
    }
    let temporary = parent.join(format!(
        ".workunit-{}-{}.tmp",
        std::process::id(),
        crate::core::ulid::new_ulid()
    ));
    let result = (|| {
        let mut file = crate::core::fs_permissions::open_storage_file(
            &temporary,
            fs::OpenOptions::new().create_new(true).write(true),
        )?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temporary);
        return Err(journal_error(
            "WORKUNIT_RECOVERY_REQUIRED",
            format!("journal committed; projection publication must be reconciled: {e}"),
        ));
    }
    crate::core::atomic::sync_parent_directory(parent).map_err(|e| {
        journal_error(
            "ARTIFACT_DURABILITY_UNCONFIRMED",
            format!("complete workunit projection preserved; reconcile before continuing: {e}"),
        )
    })?;
    Ok(path)
}

fn proof_artifact_hashes(
    root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<std::collections::BTreeMap<String, String>, error::DecapodError> {
    proof_artifact_hashes_with_limits(
        root,
        manifest,
        1024,
        32 * 1024 * 1024,
        128 * 1024 * 1024,
        std::time::Duration::from_secs(30),
    )
}

fn proof_artifact_hashes_with_limits(
    root: &Path,
    manifest: &WorkUnitManifest,
    max_unique_artifacts: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
    timeout: std::time::Duration,
) -> Result<std::collections::BTreeMap<String, String>, error::DecapodError> {
    use std::io::Read;
    use std::path::Component;
    let deadline = std::time::Instant::now() + timeout;
    let check_deadline = || {
        if std::time::Instant::now() >= deadline {
            Err(journal_error(
                "WORKUNIT_PROOF_LIMIT",
                "proof read deadline exceeded",
            ))
        } else {
            Ok(())
        }
    };
    let mut references = std::collections::BTreeSet::new();
    let mut hashes = std::collections::BTreeMap::new();
    let canonical = if let Some((candidate, record)) = manifest_validation_reference(manifest)? {
        let store = crate::core::store::Store {
            kind: crate::core::store::StoreKind::Repo,
            root: crate::core::store::find_governance_root(root).join(".decapod/data"),
        };
        let observed =
            crate::core::validation_history::lookup_record(root, &store, candidate, record)?;
        if observed.task_binding().task_id != manifest.task_id
            || manifest
                .validation_record
                .as_ref()
                .is_none_or(|reference| reference.record_digest != observed.record_digest())
        {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "canonical reference belongs to another task or changed record",
            ));
        }
        true
    } else {
        false
    };
    for proof in &manifest.proof_results {
        check_deadline()?;
        if proof.status != "pass" {
            continue;
        }
        let Some(reference) = proof.artifact_ref.as_deref() else {
            if canonical && manifest.proof_plan.contains(&proof.gate) {
                continue;
            }
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                format!("gate {} has no immutable artifact reference", proof.gate),
            ));
        };
        let relative = Path::new(reference);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "proof references must be repository-relative regular files",
            ));
        }
        references.insert(reference);
        if references.len() + hashes.len() > max_unique_artifacts {
            return Err(journal_error(
                "WORKUNIT_PROOF_LIMIT",
                "too many unique proof artifacts",
            ));
        }
    }
    if references.is_empty() {
        return Ok(hashes);
    }
    let root = root.canonicalize()?;
    // The existing epoch input owner pins every ancestor with openat/O_NOFOLLOW.
    // Confined final-file symlinks may resolve to regular content; external,
    // absolute, runtime-target and directory-link escapes remain rejected.
    // Resume rehashes that content. Never reopen an already checked string path.
    let input = crate::core::validation_epoch::input::BoundedEpochInput::new(&root)
        .map_err(|e| journal_error("WORKUNIT_PROOF_UNVERIFIABLE", e))?;
    let mut remaining_bytes = max_total_bytes;
    for reference in references {
        check_deadline()?;
        let mut file = input
            .open_regular(&root.join(reference))
            .map_err(|e| journal_error("WORKUNIT_PROOF_UNVERIFIABLE", e))?;
        let before = file.metadata()?;
        if !before.is_file() || before.len() > max_file_bytes {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "proof is not a bounded regular file",
            ));
        }
        if before.len() > remaining_bytes {
            return Err(journal_error(
                "WORKUNIT_PROOF_LIMIT",
                "total proof byte limit exceeded",
            ));
        }
        let mut hash = Sha256::new();
        let mut file_bytes = 0_u64;
        let mut chunk = [0_u8; 65536];
        loop {
            check_deadline()?;
            let count = file.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            file_bytes = file_bytes.saturating_add(count as u64);
            if file_bytes > max_file_bytes {
                return Err(journal_error(
                    "WORKUNIT_PROOF_LIMIT",
                    "individual proof byte limit exceeded",
                ));
            }
            remaining_bytes = remaining_bytes.checked_sub(count as u64).ok_or_else(|| {
                journal_error("WORKUNIT_PROOF_LIMIT", "total proof byte limit exceeded")
            })?;
            hash.update(&chunk[..count]);
        }
        let after = file.metadata()?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "proof artifact changed while reading",
            ));
        }
        hashes.insert(
            reference.to_string(),
            format!("sha256:{:x}", hash.finalize()),
        );
    }
    Ok(hashes)
}

fn verify_resume_record(
    root: &Path,
    record: &WorkUnitJournalRecord,
) -> Result<(), error::DecapodError> {
    let binding = record.binding.as_ref().ok_or_else(|| {
        journal_error(
            "WORKUNIT_CHECKPOINT_REQUIRED",
            "no current execution binding; legacy history is not reconstructed",
        )
    })?;
    if record.manifest.intent_ref != binding.intent_id {
        return Err(journal_error(
            "WORKUNIT_INTENT_CONFLICT",
            "workunit intent differs from current execution authority",
        ));
    }
    crate::core::trajectory::verify_execution_binding(root, &record.task_id, binding)?;
    if record.proof_hashes != proof_artifact_hashes(root, &record.manifest)? {
        return Err(journal_error(
            "WORKUNIT_PROOF_CHANGED",
            "referenced proof artifact bytes no longer match the checkpoint",
        ));
    }
    if record.manifest.status == WorkUnitStatus::Verified {
        validate_observed_workunit_proof(root, &record.manifest)?;
        ensure_living_specs_bound_for_proof(root, &record.manifest)?;
        if record.legacy_baseline && record.kind == "legacy_baseline" {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "legacy baseline is historical state, not proof of prior execution",
            ));
        }
    }
    Ok(())
}

fn append_snapshot(
    root: &Path,
    manifest: &WorkUnitManifest,
    operation_id: &str,
    kind: &str,
    expected_generation: u64,
    binding: Option<crate::core::trajectory::ExecutionStateBinding>,
    proof_hashes: std::collections::BTreeMap<String, String>,
) -> Result<WorkUnitJournalRecord, error::DecapodError> {
    validate_operation_id(operation_id)?;
    validate_task_id(&manifest.task_id)?;
    if serde_json::to_vec_pretty(manifest)
        .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?
        .len() as u64
        > MAX_MANIFEST_BYTES
    {
        return Err(journal_error(
            "WORKUNIT_JOURNAL_CAPACITY",
            "manifest exceeds recoverable projection byte bound",
        ));
    }
    let data = runtime_data_dir(root);
    let broker = crate::core::broker::DbBroker::new(&data);
    let actor = std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "decapod".to_string());
    broker.with_transaction(
        &crate::core::events::canonical_db_path(&data),
        &actor,
        Some(&manifest.intent_ref),
        "workunit.journal.append",
        |conn| {
            let records = journal_records_on_conn(conn, &manifest.task_id)?;
            let latest = records.last();
            if manifest.status == WorkUnitStatus::Verified
                && (binding.is_some() || !matches!(kind, "initialized" | "legacy_baseline"))
            {
                let canonical = crate::core::store::find_governance_root(root).join(".decapod/data");
                if crate::core::events::canonical_db_path(&data).canonicalize()?
                    != crate::core::events::canonical_db_path(&canonical).canonicalize()?
                {
                    return Err(journal_error("WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED",
                        "workspace-local workunit state and canonical task proof use different databases; no cross-store atomic acceptance is asserted"));
                }
                crate::core::validation_history::check_workunit_append_on_conn(conn, manifest)?;
            }
            let manifest_hash = manifest
                .canonical_hash_hex()
                .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?;
            if let Some(existing) = records
                .iter()
                .find(|record| record.operation_id == operation_id)
            {
                if existing.kind == kind
                    && existing.manifest_hash == manifest_hash
                    && existing.generation == expected_generation.saturating_add(1)
                    && existing.binding == binding
                    && existing.proof_hashes == proof_hashes
                {
                    return Ok(existing.clone());
                }
                return Err(journal_error(
                    "WORKUNIT_OPERATION_CONFLICT",
                    "operation ID already binds different request bytes or predecessor",
                ));
            }
            if latest.map_or(0, |record| record.generation) != expected_generation {
                return Err(journal_error(
                    "WORKUNIT_GENERATION_CONFLICT",
                    "workunit changed after the caller prepared its update",
                ));
            }
            let mut record = WorkUnitJournalRecord {
                schema_version: JOURNAL_SCHEMA.into(),
                event_type: JOURNAL_EVENT.into(),
                event_id: operation_event_id(&manifest.task_id, operation_id),
                ts: crate::core::time::now_epoch_z(),
                actor: actor.clone(),
                task_id: manifest.task_id.clone(),
                operation_id: operation_id.into(),
                kind: kind.into(),
                generation: expected_generation.checked_add(1).ok_or_else(|| {
                    journal_error("WORKUNIT_JOURNAL_INVALID", "generation overflow")
                })?,
                predecessor: latest.map(|record| record.record_hash.clone()),
                manifest_hash,
                manifest: manifest.canonicalized(),
                binding,
                proof_hashes,
                legacy_baseline: latest
                    .map_or(kind == "legacy_baseline", |record| record.legacy_baseline),
                record_hash: String::new(),
            };
            record.record_hash = record_hash(&record)?;
            let payload = serde_json::to_value(&record)
                .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?;
            crate::core::events::check_subject_append_capacity(
                conn,
                crate::core::events::WORKUNIT,
                "task",
                &manifest.task_id,
                &payload,
                10_000,
            )?;
            crate::core::events::append_on_conn(conn, crate::core::events::WORKUNIT, &payload)?;
            Ok(record)
        },
    )
}

fn baseline_if_needed(
    root: &Path,
    task_id: &str,
) -> Result<WorkUnitJournalRecord, error::DecapodError> {
    if let Some(record) = journal_records(root, task_id)?.pop() {
        return Ok(record);
    }
    let manifest = load_workunit_projection(root, task_id)?;
    let id = format!(
        "legacy-baseline:{}",
        manifest
            .canonical_hash_hex()
            .map_err(|e| journal_error("WORKUNIT_JOURNAL_INVALID", e))?
    );
    append_snapshot(
        root,
        &manifest,
        &id,
        "legacy_baseline",
        0,
        None,
        Default::default(),
    )
}

fn mutate_workunit(
    root: &Path,
    task_id: &str,
    mutate: impl FnOnce(&mut WorkUnitManifest) -> Result<(), error::DecapodError>,
) -> Result<WorkUnitManifest, error::DecapodError> {
    mutate_workunit_with_proof_gate(root, task_id, None, mutate)
}

/// Only record_proof_result owns permission to capture new proof bytes, and
/// only for its explicit gate. Unrelated updates cannot refresh old evidence.
fn verify_retained_proof_hashes(
    previous: &WorkUnitManifest,
    current: &WorkUnitManifest,
    old_hashes: &std::collections::BTreeMap<String, String>,
    new_hashes: &std::collections::BTreeMap<String, String>,
    updated_gate: Option<&str>,
) -> Result<(), error::DecapodError> {
    for proof in current
        .proof_results
        .iter()
        .filter(|proof| proof.status == "pass")
    {
        if Some(proof.gate.as_str()) == updated_gate {
            continue;
        }
        let Some(reference) = proof.artifact_ref.as_ref() else {
            if current.validation_record.is_some()
                && current.validation_record == previous.validation_record
                && current.proof_plan.contains(&proof.gate)
                && previous.proof_results.contains(proof)
            {
                continue;
            }
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "passing declaration has no artifact or retained canonical observation",
            ));
        };
        if !previous.proof_results.contains(proof)
            || !old_hashes.contains_key(reference)
            || old_hashes.get(reference) != new_hashes.get(reference)
        {
            return Err(journal_error(
                "WORKUNIT_PROOF_CHANGED",
                format!(
                    "retained gate {} no longer matches its recorded proof bytes",
                    proof.gate
                ),
            ));
        }
    }
    Ok(())
}

fn mutate_workunit_with_proof_gate(
    root: &Path,
    task_id: &str,
    updated_gate: Option<&str>,
    mutate: impl FnOnce(&mut WorkUnitManifest) -> Result<(), error::DecapodError>,
) -> Result<WorkUnitManifest, error::DecapodError> {
    with_workunit_lock(root, || {
        let previous = baseline_if_needed(root, task_id)?;
        if !projection_matches(root, &previous)? {
            return Err(journal_error(
                "WORKUNIT_RECOVERY_REQUIRED",
                "projection differs from the durable journal",
            ));
        }
        let mut manifest = previous.manifest.clone();
        mutate(&mut manifest)?;
        let mut proof_hashes = previous.proof_hashes.clone();
        if (manifest.proof_results != previous.manifest.proof_results || updated_gate.is_some())
            && let Some(binding) = previous.binding.as_ref()
        {
            // New proof may be recorded only under the current execution
            // binding. Clearing obsolete proofs is always a safe downgrade.
            if manifest
                .proof_results
                .iter()
                .any(|proof| proof.status == "pass")
            {
                crate::core::trajectory::verify_execution_binding(root, task_id, binding)?;
            }
            proof_hashes = proof_artifact_hashes(root, &manifest)?;
            verify_retained_proof_hashes(
                &previous.manifest,
                &manifest,
                &previous.proof_hashes,
                &proof_hashes,
                updated_gate,
            )?;
        }
        if manifest.status == WorkUnitStatus::Verified {
            if previous.binding.is_none() {
                return Err(journal_error(
                    "WORKUNIT_CHECKPOINT_REQUIRED",
                    "VERIFIED requires a current execution checkpoint",
                ));
            }
            validate_observed_workunit_proof(root, &manifest)?;
            ensure_living_specs_bound_for_proof(root, &manifest)?;
        }
        if manifest.status == WorkUnitStatus::Verified
            && let Some(binding) = previous.binding.as_ref()
        {
            crate::core::trajectory::verify_execution_binding(root, task_id, binding)?;
            if proof_hashes != proof_artifact_hashes(root, &manifest)? {
                return Err(journal_error(
                    "WORKUNIT_PROOF_CHANGED",
                    "proof bytes differ from the execution checkpoint",
                ));
            }
        }
        if manifest.canonicalized() == previous.manifest && proof_hashes == previous.proof_hashes {
            return Ok(previous.manifest);
        }
        let record = append_snapshot(
            root,
            &manifest,
            &crate::core::ulid::new_ulid(),
            "mutation",
            previous.generation,
            previous.binding,
            proof_hashes,
        )?;
        publish_projection(root, &record.manifest)?;
        Ok(record.manifest)
    })
}

/// Read-only journal/projection reconciliation. No migration, adoption or proof grant.
pub fn workunit_journal_status(
    root: &Path,
    task_id: &str,
) -> Result<WorkUnitJournalStatus, error::DecapodError> {
    let records = journal_records(root, task_id)?;
    let Some(record) = records.last() else {
        let exists = workunit_path(root, task_id)?.exists()
            || root
                .join(".decapod/governance/workunits")
                .join(format!("{task_id}.json"))
                .exists();
        return Ok(WorkUnitJournalStatus {
            task_id: task_id.into(),
            generation: None,
            checkpoint_id: None,
            checkpoint_hash: None,
            projection_state: if exists {
                "legacy_unjournaled"
            } else {
                "absent"
            }
            .into(),
            legacy_baseline: exists,
            resumable: false,
            resume_blocker: Some("WORKUNIT_CHECKPOINT_REQUIRED".into()),
        });
    };
    let projection_state = match load_workunit_projection(root, task_id) {
        Ok(manifest)
            if manifest.canonical_hash_hex().ok().as_deref()
                == Some(record.manifest_hash.as_str()) =>
        {
            "current"
        }
        Ok(_) => "recovery_required",
        Err(error::DecapodError::NotFound(_)) => "missing",
        Err(_) => "invalid",
    };
    let resume_blocker = verify_resume_record(root, record)
        .err()
        .map(|e| e.to_string());
    Ok(WorkUnitJournalStatus {
        task_id: task_id.into(),
        generation: Some(record.generation),
        checkpoint_id: Some(record.operation_id.clone()),
        checkpoint_hash: Some(record.record_hash.clone()),
        projection_state: projection_state.into(),
        legacy_baseline: record.legacy_baseline,
        resumable: projection_state == "current" && resume_blocker.is_none(),
        resume_blocker,
    })
}

/// Explicitly checkpoint current material. A legacy baseline preserves history
/// but cannot make previously unbound VERIFIED evidence current.
pub fn checkpoint_workunit(
    root: &Path,
    task_id: &str,
    operation_id: &str,
    expected_generation: Option<u64>,
) -> Result<WorkUnitJournalStatus, error::DecapodError> {
    with_workunit_lock(root, || {
        let previous = baseline_if_needed(root, task_id)?;
        let records = journal_records(root, task_id)?;
        if let Some(existing) = records
            .iter()
            .find(|record| record.operation_id == operation_id)
        {
            if existing.kind != "checkpoint"
                || expected_generation
                    .is_some_and(|generation| generation.saturating_add(1) != existing.generation)
            {
                return Err(journal_error(
                    "WORKUNIT_OPERATION_CONFLICT",
                    "checkpoint ID already binds another operation",
                ));
            }
            verify_resume_record(root, existing)?;
            if existing.generation != previous.generation {
                return Err(journal_error(
                    "WORKUNIT_GENERATION_CONFLICT",
                    "checkpoint was superseded",
                ));
            }
            if !projection_matches(root, existing).unwrap_or(false) {
                return Err(journal_error(
                    "WORKUNIT_RECOVERY_REQUIRED",
                    "recover the checkpoint projection explicitly",
                ));
            }
            return workunit_journal_status(root, task_id);
        }
        if !projection_matches(root, &previous)? {
            return Err(journal_error(
                "WORKUNIT_RECOVERY_REQUIRED",
                "recover the projection before checkpointing",
            ));
        }
        if previous.manifest.status == WorkUnitStatus::Verified
            || previous
                .manifest
                .proof_results
                .iter()
                .any(|proof| proof.status == "pass")
            || !previous.proof_hashes.is_empty()
        {
            verify_resume_record(root, &previous)?;
        }
        let binding = crate::core::trajectory::capture_execution_binding(root, task_id)?;
        if previous.manifest.intent_ref != binding.intent_id {
            return Err(journal_error(
                "WORKUNIT_INTENT_CONFLICT",
                "workunit intent differs from current execution authority",
            ));
        }
        let hashes = proof_artifact_hashes(root, &previous.manifest)?;
        let record = append_snapshot(
            root,
            &previous.manifest,
            operation_id,
            "checkpoint",
            expected_generation.unwrap_or(previous.generation),
            Some(binding),
            hashes,
        )?;
        publish_projection(root, &record.manifest)?;
        workunit_journal_status(root, task_id)
    })
}

/// Restore only a known durable projection. Successful recovery is not resume,
/// validation, publication, or proof success.
pub fn recover_workunit(
    root: &Path,
    task_id: &str,
) -> Result<WorkUnitJournalStatus, error::DecapodError> {
    with_workunit_lock(root, || {
        let records = journal_records(root, task_id)?;
        let record = records.last().ok_or_else(|| {
            journal_error(
                "WORKUNIT_CHECKPOINT_REQUIRED",
                "no journal; explicitly checkpoint legacy state first",
            )
        })?;
        if projection_matches(root, record).unwrap_or(false) {
            return workunit_journal_status(root, task_id);
        }
        if let Ok(projected) = load_workunit_projection(root, task_id) {
            let digest = projected
                .canonical_hash_hex()
                .map_err(|e| journal_error("WORKUNIT_PROJECTION_INVALID", e))?;
            if !records.iter().any(|prior| prior.manifest_hash == digest) {
                return Err(journal_error(
                    "WORKUNIT_PROJECTION_CONFLICT",
                    "unrecognized valid file contents were preserved; resolve explicitly",
                ));
            }
        }
        if record.manifest.status == WorkUnitStatus::Verified {
            verify_resume_record(root, record)?;
        }
        publish_projection(root, &record.manifest)?;
        workunit_journal_status(root, task_id)
    })
}

pub fn resume_workunit(
    root: &Path,
    task_id: &str,
    operation_id: &str,
    expected_generation: Option<u64>,
) -> Result<WorkUnitJournalStatus, error::DecapodError> {
    with_workunit_lock(root, || {
        let records = journal_records(root, task_id)?;
        let previous = records.last().ok_or_else(|| {
            journal_error(
                "WORKUNIT_CHECKPOINT_REQUIRED",
                "checkpoint current execution before resume",
            )
        })?;
        verify_resume_record(root, previous)?;
        if let Some(existing) = records
            .iter()
            .find(|record| record.operation_id == operation_id)
        {
            if existing.kind != "resumed"
                || existing.generation != previous.generation
                || expected_generation
                    .is_some_and(|generation| generation.saturating_add(1) != existing.generation)
            {
                return Err(journal_error(
                    "WORKUNIT_OPERATION_CONFLICT",
                    "resume ID was reused or superseded",
                ));
            }
            if !projection_matches(root, previous).unwrap_or(false) {
                return Err(journal_error(
                    "WORKUNIT_RECOVERY_REQUIRED",
                    "recover the resumed projection explicitly",
                ));
            }
            return workunit_journal_status(root, task_id);
        }
        if !projection_matches(root, previous)? {
            return Err(journal_error(
                "WORKUNIT_RECOVERY_REQUIRED",
                "recover before resuming",
            ));
        }
        let record = append_snapshot(
            root,
            &previous.manifest,
            operation_id,
            "resumed",
            expected_generation.unwrap_or(previous.generation),
            previous.binding.clone(),
            previous.proof_hashes.clone(),
        )?;
        publish_projection(root, &record.manifest)?;
        workunit_journal_status(root, task_id)
    })
}

/// Explicit compare-and-swap for externally prepared manifest snapshots.
pub fn write_workunit_checked(
    root: &Path,
    manifest: &WorkUnitManifest,
    operation_id: &str,
    expected_generation: u64,
) -> Result<PathBuf, error::DecapodError> {
    with_workunit_lock(root, || {
        let previous = baseline_if_needed(root, &manifest.task_id)?;
        if manifest.intent_ref != previous.manifest.intent_ref
            || manifest.proof_plan != previous.manifest.proof_plan
            || manifest.proof_results != previous.manifest.proof_results
            || manifest.validation_epoch != previous.manifest.validation_epoch
            || manifest.validation_record != previous.manifest.validation_record
        {
            return Err(journal_error(
                "WORKUNIT_DOMAIN_TRANSITION_REQUIRED",
                "prepared snapshots cannot alter intent or proof authority; use the owned proof/transition operations",
            ));
        }
        if manifest.status != previous.manifest.status
            && (manifest.status == WorkUnitStatus::Verified
                || previous.manifest.status == WorkUnitStatus::Verified)
        {
            return Err(journal_error(
                "WORKUNIT_DOMAIN_TRANSITION_REQUIRED",
                "prepared snapshots cannot cross VERIFIED; use the owned status transition so proof custody is checked or cleared",
            ));
        }
        if !can_transition(&previous.manifest.status, &manifest.status) {
            return Err(journal_error(
                "WORKUNIT_DOMAIN_TRANSITION_REQUIRED",
                "invalid prepared status transition",
            ));
        }
        if manifest.status == WorkUnitStatus::Verified {
            verify_resume_record(root, &previous)?;
            validate_observed_workunit_proof(root, manifest)?;
            ensure_living_specs_bound_for_proof(root, manifest)?;
        }
        if !projection_matches(root, &previous)? {
            return Err(journal_error(
                "WORKUNIT_RECOVERY_REQUIRED",
                "recover before writing",
            ));
        }
        let record = append_snapshot(
            root,
            manifest,
            operation_id,
            "mutation",
            expected_generation,
            previous.binding,
            previous.proof_hashes,
        )?;
        // Retrying an older successful operation must never rewind newer state.
        let latest = journal_records(root, &manifest.task_id)?
            .pop()
            .ok_or_else(|| journal_error("WORKUNIT_JOURNAL_INVALID", "committed record missing"))?;
        if latest.generation != record.generation {
            return Err(journal_error(
                "WORKUNIT_GENERATION_CONFLICT",
                "operation already succeeded but was superseded",
            ));
        }
        publish_projection(root, &record.manifest)
    })
}

/// Initialize in an existing repository runtime (`decapod init` first).
/// Missing policy schema is an explicit error and is never silently defaulted.
pub fn init_workunit(
    project_root: &Path,
    task_id: &str,
    intent_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    with_workunit_lock(project_root, || {
        if workunit_path(project_root, task_id)?.exists()
            || !journal_records(project_root, task_id)?.is_empty()
        {
            return Err(error::DecapodError::ValidationError(format!(
                "workunit '{task_id}' already exists"
            )));
        }
        let manifest = WorkUnitManifest {
            task_id: task_id.into(),
            intent_ref: intent_ref.into(),
            spec_refs: vec![],
            state_refs: vec![],
            proof_plan: vec![],
            proof_results: vec![],
            validation_epoch: None,
            validation_record: None,
            status: WorkUnitStatus::Draft,
        };
        let record = append_snapshot(
            project_root,
            &manifest,
            &crate::core::ulid::new_ulid(),
            "initialized",
            0,
            None,
            Default::default(),
        )?;
        publish_projection(project_root, &record.manifest)?;
        Ok(record.manifest)
    })
}

pub fn load_workunit(
    project_root: &Path,
    task_id: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let records = journal_records(project_root, task_id)?;
    if let Some(record) = records.last() {
        if !projection_matches(project_root, record).unwrap_or(false) {
            return Err(journal_error(
                "WORKUNIT_RECOVERY_REQUIRED",
                "manifest projection differs from durable journal; recover explicitly",
            ));
        }
        return Ok(record.manifest.clone());
    }
    load_workunit_projection(project_root, task_id)
}

/// Compatibility writer: import first content or repeat identical content only.
/// An initialized repository runtime is required, including policy schema.
/// Prepared replacements require write_workunit_checked and the observed generation.
pub fn write_workunit(
    project_root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<PathBuf, error::DecapodError> {
    with_workunit_lock(project_root, || {
        let records = journal_records(project_root, &manifest.task_id)?;
        if let Some(record) = records.last() {
            if record.manifest != manifest.canonicalized() {
                return Err(journal_error(
                    "WORKUNIT_EXPECTED_GENERATION_REQUIRED",
                    "use write_workunit_checked for a prepared replacement",
                ));
            }
            if !projection_matches(project_root, record).unwrap_or(false) {
                return Err(journal_error(
                    "WORKUNIT_RECOVERY_REQUIRED",
                    "recover explicitly instead of overwriting a divergent projection",
                ));
            }
            return workunit_path(project_root, &manifest.task_id);
        }
        if workunit_path(project_root, &manifest.task_id)?.exists() {
            let existing = load_workunit_projection(project_root, &manifest.task_id)?;
            if existing.canonicalized() != manifest.canonicalized() {
                return Err(journal_error(
                    "WORKUNIT_EXPECTED_GENERATION_REQUIRED",
                    "legacy replacement requires an explicit baseline and generation",
                ));
            }
            baseline_if_needed(project_root, &manifest.task_id)?;
            return workunit_path(project_root, &manifest.task_id);
        }
        let record = append_snapshot(
            project_root,
            manifest,
            &crate::core::ulid::new_ulid(),
            "initialized",
            0,
            None,
            Default::default(),
        )?;
        publish_projection(project_root, &record.manifest)
    })
}

pub fn add_spec_ref(
    root: &Path,
    task_id: &str,
    spec_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    mutate_workunit(root, task_id, |manifest| {
        manifest.spec_refs.push(spec_ref.into());
        Ok(())
    })
}
pub fn add_state_ref(
    root: &Path,
    task_id: &str,
    state_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    mutate_workunit(root, task_id, |manifest| {
        manifest.state_refs.push(state_ref.into());
        Ok(())
    })
}
pub fn set_proof_plan(
    root: &Path,
    task_id: &str,
    gates: &[String],
) -> Result<WorkUnitManifest, error::DecapodError> {
    mutate_workunit(root, task_id, |manifest| {
        let mut canonical = gates.to_vec();
        canonical.sort();
        canonical.dedup();
        if manifest.proof_plan != canonical {
            manifest.proof_plan = canonical;
            manifest.proof_results.clear();
            manifest.validation_epoch = None;
            manifest.validation_record = None;
            if manifest.status == WorkUnitStatus::Verified {
                manifest.status = WorkUnitStatus::Claimed;
            }
        }
        Ok(())
    })
}
pub fn record_proof_result(
    root: &Path,
    task_id: &str,
    gate: &str,
    status: &str,
    artifact_ref: Option<String>,
) -> Result<WorkUnitManifest, error::DecapodError> {
    if !matches!(status, "pass" | "fail") {
        return Err(error::DecapodError::ValidationError(format!(
            "invalid proof status '{status}': expected pass|fail"
        )));
    }
    mutate_workunit_with_proof_gate(root, task_id, Some(gate), |manifest| {
        if manifest.status == WorkUnitStatus::Verified {
            manifest.status = WorkUnitStatus::Claimed;
        }
        // All caller-supplied outcomes remain declarations, including pass.
        // Remove current derived rows when reopening a canonical observation;
        // their original successful observation remains in history and journal.
        if manifest.validation_record.take().is_some() {
            manifest
                .proof_results
                .retain(|proof| !manifest.proof_plan.contains(&proof.gate));
        }
        manifest.validation_epoch = None;
        manifest.proof_results.retain(|result| result.gate != gate);
        manifest.proof_results.push(WorkUnitProofResult {
            gate: gate.into(),
            status: status.into(),
            artifact_ref,
            evaluator_epoch: None,
            validation_epoch: None,
        });
        Ok(())
    })
}
pub fn transition_status(
    root: &Path,
    task_id: &str,
    to: WorkUnitStatus,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let gate = if to == WorkUnitStatus::Verified {
        let manifest = load_workunit(root, task_id)?;
        crate::core::validation_history::ensure_full_validation_plan(&manifest.proof_plan)?;
        Some(manifest.proof_plan[0].clone())
    } else {
        None
    };
    mutate_workunit_with_proof_gate(root, task_id, gate.as_deref(), |manifest| {
        if !can_transition(&manifest.status, &to) {
            return Err(error::DecapodError::ValidationError(format!(
                "invalid workunit transition: {:?} -> {to:?}",
                manifest.status,
            )));
        }
        if to == WorkUnitStatus::Verified {
            if journal_records(root, task_id)?
                .last()
                .is_none_or(|record| record.binding.is_none())
            {
                return Err(journal_error(
                    "WORKUNIT_CHECKPOINT_REQUIRED",
                    "VERIFIED requires a current execution checkpoint",
                ));
            }
            crate::core::validation_history::ensure_full_validation_plan(&manifest.proof_plan)?;
            if gate.as_deref() != Some(manifest.proof_plan[0].as_str()) {
                return Err(journal_error(
                    "WORKUNIT_GENERATION_CONFLICT",
                    "proof requirements changed before transition",
                ));
            }
            let observed = crate::core::validation_history::current_workunit_proof(
                root,
                manifest,
                manifest_validation_reference(manifest)?,
            )?;
            let epoch = observed.validation_epoch().clone();
            let gate = manifest.proof_plan[0].clone();
            manifest.proof_results.retain(|proof| proof.gate != gate);
            manifest.proof_results.push(WorkUnitProofResult {
                gate,
                status: "pass".into(),
                artifact_ref: None,
                evaluator_epoch: Some(epoch.epoch_id.clone()),
                validation_epoch: Some(epoch.clone()),
            });
            manifest.validation_epoch = Some(epoch);
            manifest.validation_record = Some(WorkUnitValidationReference {
                candidate_sha: observed.candidate_sha().into(),
                record_id: observed.record_id().into(),
                record_digest: observed.record_digest().into(),
            });
            ensure_living_specs_bound_for_proof(root, manifest)?;
        } else if to == WorkUnitStatus::Claimed && manifest.status == WorkUnitStatus::Verified {
            // Explicit safe downgrade: historical observations remain in the
            // journal; the current claim must obtain fresh observed proof.
            manifest.proof_results.clear();
            manifest.validation_epoch = None;
            manifest.validation_record = None;
        }
        manifest.status = to;
        Ok(())
    })
}

/// Validate the current durable journal, execution custody and canonical proof.
/// A structurally valid legacy projection cannot stand in for observed execution.
pub fn validate_verified_manifest(
    root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<(), error::DecapodError> {
    let records = journal_records(root, &manifest.task_id)?;
    let current = records.last().ok_or_else(|| {
        journal_error(
            "WORKUNIT_CHECKPOINT_REQUIRED",
            "no durable execution checkpoint for this workunit",
        )
    })?;
    if current.manifest != manifest.canonicalized() || !projection_matches(root, current)? {
        return Err(journal_error(
            "WORKUNIT_RECOVERY_REQUIRED",
            "manifest differs from the current durable checkpoint",
        ));
    }
    if manifest.status != WorkUnitStatus::Verified {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "current status is not VERIFIED",
        ));
    }
    verify_resume_record(root, current)
}

// Used before atomically appending a new transition. Public consumers also
// require the resulting durable snapshot through validate_verified_manifest.
fn validate_observed_workunit_proof(
    root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<(), error::DecapodError> {
    if manifest.status != WorkUnitStatus::Verified {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "current status is not VERIFIED",
        ));
    }
    ensure_verified_ready(manifest)?;
    let observed = crate::core::validation_history::current_workunit_proof(
        root,
        manifest,
        manifest_validation_reference(manifest)?,
    )?;
    if manifest.validation_epoch.as_ref() != Some(observed.validation_epoch())
        || manifest
            .validation_record
            .as_ref()
            .is_none_or(|reference| reference.record_digest != observed.record_digest())
    {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "workunit epoch does not match canonical observed proof",
        ));
    }
    Ok(())
}

/// Living specs are evidence material for proof completion (GitHub #1183).
///
/// When the repository has a living-specs surface, a VERIFIED workunit must
/// reference at least one `.decapod/managed/specs/*` path so completion evidence
/// cannot omit the project contract.
pub fn ensure_living_specs_bound_for_proof(
    project_root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<(), error::DecapodError> {
    use crate::core::project_specs::LOCAL_PROJECT_SPECS;

    let has_living_specs_surface = LOCAL_PROJECT_SPECS
        .iter()
        .any(|spec| project_root.join(spec.path).is_file());
    if !has_living_specs_surface {
        return Ok(());
    }

    let bound = manifest.spec_refs.iter().any(|spec_ref| {
        let normalized = spec_ref.replace('\\', "/");
        normalized.contains(".decapod/managed/specs/")
            || LOCAL_PROJECT_SPECS.iter().any(|spec| {
                normalized == spec.path
                    || normalized.ends_with(&format!("/{}", spec.path))
                    || normalized.ends_with(spec.path.trim_start_matches(".decapod/managed/specs/"))
            })
    });
    if bound {
        return Ok(());
    }

    Err(error::DecapodError::ValidationError(
        "cannot transition to VERIFIED: living specs are evidence material for proof completion; \
bind at least one `.decapod/managed/specs/*` path via workunit spec_refs (fingerprint/attestation \
refresh alone is not a living-spec rewrite — see FINGERPRINT_ONLY_SPECS / #1183)"
            .to_string(),
    ))
}

fn can_transition(from: &WorkUnitStatus, to: &WorkUnitStatus) -> bool {
    use WorkUnitStatus::*;
    matches!(
        (from, to),
        (Draft, Executing)
            | (Executing, Claimed)
            | (Claimed, Verified)
            | (Executing, Draft)
            | (Draft, Draft)
            | (Executing, Executing)
            | (Claimed, Claimed)
            | (Verified, Verified)
            | (Verified, Claimed)
    )
}

fn ensure_verified_ready(manifest: &WorkUnitManifest) -> Result<(), error::DecapodError> {
    if manifest.proof_plan.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "cannot transition to VERIFIED without proof_plan gates".into(),
        ));
    }
    for gate in &manifest.proof_plan {
        let proofs: Vec<_> = manifest
            .proof_results
            .iter()
            .filter(|proof| &proof.gate == gate)
            .collect();
        if proofs.len() != 1 || proofs[0].status != "pass" {
            return Err(error::DecapodError::ValidationError(format!(
                "cannot transition to VERIFIED: missing passing proof result for gate '{gate}'"
            )));
        }
        let epoch = manifest.validation_epoch.as_ref().ok_or_else(|| journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE", "declared pass rows have no observed validation epoch; obtain fresh full validation",
        ))?;
        let proof = proofs[0];
        if proof.validation_epoch.as_ref() != Some(epoch)
            || proof.evaluator_epoch.as_deref() != Some(epoch.epoch_id.as_str())
            || manifest.validation_record.is_none()
        {
            return Err(journal_error(
                "WORKUNIT_PROOF_UNVERIFIABLE",
                "required gate is a declaration without canonical observed provenance",
            ));
        }
    }
    crate::core::validation_history::ensure_full_validation_plan(&manifest.proof_plan)
}

pub fn verify_capsule_policy_lineage_for_task(
    project_root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<(), error::DecapodError> {
    let task_id = manifest.task_id.as_str();
    let expected_rel = format!(".decapod/managed/context/{task_id}.json");
    let expected_abs = project_root
        .join(&expected_rel)
        .to_string_lossy()
        .to_string()
        .replace('\\', "/");
    let expected_rel_norm = expected_rel.replace('\\', "/");
    let has_capsule_state_ref = manifest.state_refs.iter().any(|state_ref| {
        let normalized = state_ref.replace('\\', "/");
        normalized == expected_rel_norm
            || normalized == expected_abs
            || normalized.ends_with(&format!("/{expected_rel_norm}"))
    });
    if !has_capsule_state_ref {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_STATE_REF_MISSING: expected state_ref '{expected_rel}' for task '{task_id}'"
        )));
    }

    let capsule_path = project_root
        .join(".decapod")
        .join("managed")
        .join("context")
        .join(format!("{task_id}.json"));
    if !capsule_path.exists() {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_MISSING: expected context capsule for task '{}' at {}",
            task_id,
            capsule_path.display()
        )));
    }

    let raw = fs::read_to_string(&capsule_path).map_err(error::DecapodError::IoError)?;
    let capsule: DeterministicContextCapsule = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: invalid capsule JSON at {}: {}",
            capsule_path.display(),
            e
        ))
    })?;
    let expected_hash = capsule.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: hash compute failed at {}: {}",
            capsule_path.display(),
            e
        ))
    })?;
    if capsule.capsule_hash != expected_hash {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: capsule hash mismatch at {}",
            capsule_path.display()
        )));
    }

    let task_match = capsule.task_id.as_deref() == Some(task_id)
        || capsule.workunit_id.as_deref() == Some(task_id);
    if !task_match {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: capsule task/workunit binding mismatch for task '{task_id}'"
        )));
    }

    let policy = capsule.policy;
    if policy.risk_tier.trim().is_empty()
        || policy.policy_hash.trim().is_empty()
        || policy.policy_version.trim().is_empty()
        || policy.policy_path.trim().is_empty()
        || policy.repo_revision.trim().is_empty()
    {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: missing policy lineage fields in {}",
            capsule_path.display()
        )));
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/workunit_journal_tests.rs"]
pub(crate) mod journal_tests;
