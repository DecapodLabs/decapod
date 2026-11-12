//! Durable, commit-addressed history of the live validator's complete proof.
//!
//! A record is an observation about its original candidate, never authority for
//! another publication. Only the live runtime can produce the opaque completion
//! value; neither a caller-supplied path nor the legacy governance validation
//! section can import success. The canonical verification stream owns retention.

use super::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    broker::DbBroker,
    db::{self, Connection},
    error::DecapodError,
    events,
    external_action::{self, ExternalCapability},
    laya_readiness,
    readiness_evidence::{self as evidence, ExpectedPlan, MaterialIdentity, SuiteEvidence},
    store::Store,
    todo::{self, HistoricalTaskBinding},
    validation_epoch::{self, ValidationEpochMetadata, input::BoundedEpochInput},
    validation_runtime::{self, ValidationReport},
    workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};

const SCHEMA: &str = "decapod.validation-history.v1";
const EVENT_TYPE: &str = "validation.completed";
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;
const GOVERNANCE: &str = ".decapod/governance.json";
pub(crate) const PROOF_FILE: &str = "historical-proof.json";

fn invalid(detail: impl std::fmt::Display) -> DecapodError {
    DecapodError::ValidationError(format!("VALIDATION_HISTORY: {detail}"))
}
fn digest<T: Serialize>(value: &T) -> Result<String, DecapodError> {
    evidence::digest(value).map_err(invalid)
}
fn full_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn require_sha(value: &str) -> Result<(), DecapodError> {
    if full_sha(value) {
        Ok(())
    } else {
        Err(invalid(
            "a full lowercase 40- or 64-character commit SHA is required",
        ))
    }
}
fn read_bounded(root: &Path, path: &Path) -> Result<Vec<u8>, DecapodError> {
    let input = BoundedEpochInput::new(root)?;
    let file = input.open_regular(path)?;
    if file.metadata()?.len() > MAX_RECORD_BYTES {
        return Err(invalid("proof input exceeds byte bound"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(invalid("proof input changed or exceeds byte bound"));
    }
    Ok(bytes)
}
fn governance(root: &Path) -> Result<(String, String, Value), DecapodError> {
    let bytes = read_bounded(root, &root.join(GOVERNANCE))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(invalid)?;
    let trajectory = value
        .pointer("/sections/trajectory")
        .filter(|value| value.is_object())
        .ok_or_else(|| invalid("the original task trajectory is missing"))?;
    Ok((
        format!("sha256:{:x}", Sha256::digest(&bytes)),
        digest(trajectory)?,
        trajectory.clone(),
    ))
}

fn ambient_git_override(name: &OsStr, value: &OsStr) -> bool {
    // The reviewed source runner disables ambient config with these exact
    // values. No path normalization or broader Git-config exception is safe.
    const INERT_HARDENING: &[(&str, &str)] = &[
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_COUNT", "0"),
    ];
    if INERT_HARDENING
        .iter()
        .any(|&(key, expected)| name == OsStr::new(key) && value == OsStr::new(expected))
    {
        return false;
    }
    const OVERRIDES: &[&str] = &[
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_EXEC_PATH",
        "GIT_NAMESPACE",
        "GIT_REPLACE_REF_BASE",
        "GIT_GRAFT_FILE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ];
    let name = name.to_string_lossy();
    OVERRIDES.contains(&name.as_ref()) || name.starts_with("GIT_CONFIG_")
}

fn repository_identity(
    root: &Path,
) -> Result<super::repo_identity::RepositoryIdentity, DecapodError> {
    // The shared remote resolver predates the bounded Git reader. Reject
    // ambient redirects before calling it, so its repository and our immutable
    // object observations cannot silently describe different checkouts.
    if std::env::vars_os().any(|(name, value)| ambient_git_override(&name, &value)) {
        return Err(invalid(
            "ambient Git repository or configuration overrides must be removed before historical proof use",
        ));
    }
    let trusted = crate::readiness_installed_tool(root, "git")?;
    let resolved = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .find_map(|path| {
            let path = path.join("git");
            path.is_file().then(|| path.canonicalize().ok()).flatten()
        });
    if resolved.as_ref() != Some(&trusted) {
        return Err(invalid(
            "shared repository resolver requires the same installed Git as the proof reader",
        ));
    }
    workspace::resolve_readiness_repository(root)
}

/// Captured before any validation execution, not reconstructed from its result.
#[derive(Debug, Clone)]
pub(crate) struct ValidationBinding {
    repository: String,
    candidate_sha: String,
    governance_digest: String,
    trajectory_digest: String,
    trajectory_run_id: String,
    task: HistoricalTaskBinding,
    epoch: ValidationEpochMetadata,
}

pub(crate) fn capture_task_binding(
    store: &Store,
    root: &Path,
) -> Result<Option<ValidationBinding>, DecapodError> {
    // Cloud readiness retains its existing authority. The local history
    // feature makes no unsupported claim about cloud journal durability.
    if history_is_cloud(store, root)? {
        return Ok(None);
    }
    let repository = repository_identity(root)?.canonical_name;
    let candidate_sha = git_text(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    require_sha(&candidate_sha)?;
    let (governance_digest, trajectory_digest, trajectory) = governance(root)?;
    let task_id = trajectory
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("full validation requires a task-bound trajectory"))?;
    let trajectory_run_id = trajectory
        .get("run_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("trajectory run identity is missing"))?
        .to_owned();
    let task = todo::capture_validation_binding(&store.root, task_id)?;
    let epoch = validation_epoch::active_validation_epoch_bounded(root)?;
    Ok(Some(ValidationBinding {
        repository,
        candidate_sha,
        governance_digest,
        trajectory_digest,
        trajectory_run_id,
        task,
        epoch,
    }))
}

/// Owned by ValidationReport and skipped in its public JSON. The constructor is
/// private to the runtime's successful proof path; deserialization alone is not
/// a way to mint a report or append a canonical completion record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedReadinessEvidence {
    schema: String,
    decapod_version: String,
    invocation_id: String,
    scope: String,
    material: MaterialIdentity,
    governance_digest: String,
    trajectory_digest: String,
    expected_plan: ExpectedPlan,
    suite: SuiteEvidence,
    laya: laya_readiness::Receipt,
}

impl CompletedReadinessEvidence {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_runtime(
        invocation_id: &str,
        scope: &str,
        material: &MaterialIdentity,
        governance_digest: &str,
        trajectory_digest: &str,
        expected_plan: &ExpectedPlan,
        suite: &SuiteEvidence,
        laya: &laya_readiness::Receipt,
    ) -> Result<Self, DecapodError> {
        let value = Self {
            schema: SCHEMA.into(),
            decapod_version: env!("CARGO_PKG_VERSION").into(),
            invocation_id: invocation_id.into(),
            scope: scope.into(),
            material: material.clone(),
            governance_digest: governance_digest.into(),
            trajectory_digest: trajectory_digest.into(),
            expected_plan: expected_plan.clone(),
            suite: suite.clone(),
            laya: laya.clone(),
        };
        value.verify()?;
        Ok(value)
    }
    pub(crate) fn changed_paths(&self) -> &std::collections::BTreeSet<String> {
        &self.suite.facts.changed_paths
    }
    pub(crate) fn verify_current(
        &self,
        material: &MaterialIdentity,
        governance_digest: &str,
        trajectory_digest: &str,
    ) -> Result<(), DecapodError> {
        self.verify()?;
        if &self.material != material
            || self.governance_digest != governance_digest
            || self.trajectory_digest != trajectory_digest
        {
            return Err(invalid(
                "terminal proof is bound to stale source or governance",
            ));
        }
        Ok(())
    }
    pub(crate) fn verify(&self) -> Result<(), DecapodError> {
        let local = self
            .expected_plan
            .checks
            .iter()
            .any(|check| check.kind == evidence::CheckKind::RepositoryLocal);
        let scope = if local {
            super::repository_profile::READINESS_SCOPE
        } else {
            validation_runtime::READINESS_SCOPE
        };
        if self.schema != SCHEMA
            || self.decapod_version.is_empty()
            || self.invocation_id.is_empty()
            || self.scope != scope
            || self.governance_digest.is_empty()
            || self.trajectory_digest.is_empty()
        {
            return Err(invalid("unsupported or incomplete full-readiness record"));
        }
        let verified = evidence::verify(
            &self.expected_plan,
            &self.material,
            &self.suite.facts,
            &self.suite,
        );
        if verified.verdict != evidence::Verdict::Ready {
            return Err(invalid(format!(
                "stored complete-suite proof is invalid: {:?}",
                verified.failures
            )));
        }
        let binding = &self.laya.binding;
        if !self.laya.permits_readiness()
            || self.laya.schema != "decapod.laya-readiness.v1"
            || self.laya.decapod_version != self.decapod_version
            || binding.material != self.material
            || binding.governance_digest != self.governance_digest
            || binding.proof_plan_digest != self.expected_plan.digest().map_err(invalid)?
            || binding.evidence_digest != digest(&self.suite)?
        {
            return Err(invalid(
                "Laya decision is missing, blocking, or bound to different proof",
            ));
        }
        verify_laya_receipt(&self.laya)
    }
}

fn verify_laya_receipt(receipt: &laya_readiness::Receipt) -> Result<(), DecapodError> {
    use super::laya_protocol as wire;
    match receipt.mode {
        laya_readiness::Mode::Disabled => {
            if receipt.decision != laya_readiness::Decision::Disabled
                || receipt.request_id != digest(&receipt.binding)?
                || !receipt.attempts.is_empty()
                || receipt.judgment.is_some()
            {
                return Err(invalid(
                    "disabled Laya disposition has inconsistent runtime binding",
                ));
            }
        }
        laya_readiness::Mode::Required => {
            let manifest = receipt
                .manifest
                .as_ref()
                .ok_or_else(|| invalid("required Laya manifest is missing"))?;
            let judgment = receipt
                .judgment
                .as_ref()
                .ok_or_else(|| invalid("required Laya judgment is missing"))?;
            let response = receipt
                .attempts
                .last()
                .ok_or_else(|| invalid("required Laya response is missing"))?;
            if receipt.decision != laya_readiness::Decision::Accept
                || receipt.attempts.len() > 2
                || receipt.rubric != wire::RUBRIC
                || receipt.input_bytes == 0
                || receipt
                    .runtime_sha256
                    .as_ref()
                    .is_none_or(|value| value.is_empty())
                || digest(&response.judgment)? != digest(&receipt.judgment)?
            {
                return Err(invalid(
                    "required Laya completion is incomplete or inconsistent",
                ));
            }
            // Required request IDs also bind the frozen installation, backend
            // and attempt number. They are not digest(binding), and the live
            // producer already authenticated those request/response pairs.
            // Replay checks their retained identity and the existing numerical
            // decision contract without inventing a different request ID.
            let request = wire::Request {
                schema: wire::WORKER_SCHEMA.into(),
                rubric: receipt.rubric.clone(),
                request_id: receipt.request_id.clone(),
                input_digest: receipt.input_digest.clone(),
                manifest_digest: super::laya_assets::manifest_digest(manifest).map_err(invalid)?,
                installation: PathBuf::new(),
                backend: judgment.backend,
                state: String::new(),
            };
            if response
                .decision(&request, manifest.max_tokens)
                .map_err(invalid)?
                != wire::Decision::Accept
            {
                return Err(invalid(
                    "required Laya response does not establish its recorded acceptance",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletionRecord {
    schema: String,
    result: String,
    event_id: String,
    event_type: String,
    ts: String,
    actor: String,
    candidate_sha: String,
    task_binding: HistoricalTaskBinding,
    validation_epoch: ValidationEpochMetadata,
    proof: CompletedReadinessEvidence,
    record_digest: String,
}
impl CompletionRecord {
    fn calculated_digest(&self) -> Result<String, DecapodError> {
        let mut value = serde_json::to_value(self).map_err(invalid)?;
        value
            .as_object_mut()
            .ok_or_else(|| invalid("invalid record"))?
            .remove("record_digest");
        digest(&value)
    }
    fn verify(&self) -> Result<(), DecapodError> {
        require_sha(&self.candidate_sha)?;
        if self.schema != SCHEMA
            || self.result != "success"
            || self.event_type != EVENT_TYPE
            || self.event_id.is_empty()
            || self.event_id != self.proof.invocation_id
            || self.ts.is_empty()
            || self.actor.is_empty()
            || self.actor != self.task_binding.assignee
            || self.task_binding.task_id.is_empty()
            || self.task_binding.revision < 0
            || self.task_binding.contract_digest.is_empty()
            || self.task_binding.checkpoint_digest.is_empty()
            || self.validation_epoch.schema_version
                != validation_epoch::VALIDATION_EPOCH_SCHEMA_VERSION
            || self.validation_epoch.epoch_id.is_empty()
            || self.proof.material.head_revision != self.candidate_sha
            || self.record_digest != self.calculated_digest()?
        {
            return Err(invalid(
                "canonical completion record failed identity or integrity checks",
            ));
        }
        self.proof.verify()
    }
}

fn history_is_cloud(store: &Store, root: &Path) -> Result<bool, DecapodError> {
    let selected = crate::cli::DecapodProjectConfig::load(root)?
        .repo
        .effective_backend();
    let store_project =
        super::store::find_decapod_project_root(&store.root).unwrap_or_else(|_| root.into());
    let routed = crate::cli::DecapodProjectConfig::load(&store_project)?
        .repo
        .effective_backend();
    if selected != routed {
        return Err(invalid(
            "workspace and canonical store disagree about backend routing",
        ));
    }
    Ok(selected.is_cloud())
}
fn require_local(store: &Store, root: &Path) -> Result<(), DecapodError> {
    if history_is_cloud(store, root)? {
        Err(DecapodError::NotImplemented("VALIDATION_HISTORY_BACKEND_UNSUPPORTED: history requires the local canonical verification journal".into()))
    } else {
        Ok(())
    }
}

/// Publish success only after complete proof, stable source binding and exact
/// task revision have all been rechecked. A write failure must revoke the live
/// report's success at the caller; no partial canonical record is published.
pub(crate) fn finish_archive(
    store: &Store,
    root: &Path,
    binding: &ValidationBinding,
    report: &ValidationReport,
) -> Result<(), DecapodError> {
    if !report.ready {
        return Ok(());
    }
    require_local(store, root)?;
    if !report.proof_ready || !report.blockers.is_empty() {
        return Err(invalid("successful report lacks complete proof"));
    }
    let proof = report
        .completed_evidence
        .as_ref()
        .ok_or_else(|| invalid("live runtime completion is missing"))?;
    proof.verify()?;
    let path = report
        .evidence_path
        .as_ref()
        .ok_or_else(|| invalid("runtime evidence directory is missing"))?;
    let diagnostic: CompletedReadinessEvidence =
        serde_json::from_slice(&read_bounded(path, &path.join(PROOF_FILE))?).map_err(invalid)?;
    if digest(&diagnostic)? != digest(proof)? {
        return Err(invalid("runtime evidence changed before durable append"));
    }
    let (governance_digest, trajectory_digest, _) = governance(root)?;
    if proof.material.repository != binding.repository
        || proof.material.head_revision != binding.candidate_sha
        || proof.governance_digest != binding.governance_digest
        || proof.trajectory_digest != binding.trajectory_digest
        || (proof.laya.mode != laya_readiness::Mode::Disabled
            && proof.laya.binding.trajectory_run_id != binding.trajectory_run_id)
        || report.scope != proof.scope
        || digest(&report.laya_readiness)? != digest(&Some(&proof.laya))?
        || governance_digest != binding.governance_digest
        || trajectory_digest != binding.trajectory_digest
        || git_text(root, &["rev-parse", "--verify", "HEAD^{commit}"])? != binding.candidate_sha
        || repository_identity(root)?.canonical_name != binding.repository
        || validation_epoch::active_validation_epoch_bounded(root)? != binding.epoch
    {
        return Err(invalid(
            "candidate, governance, repository, or epoch changed during validation",
        ));
    }
    let mut record = CompletionRecord {
        schema: SCHEMA.into(),
        result: "success".into(),
        event_id: proof.invocation_id.clone(),
        event_type: EVENT_TYPE.into(),
        ts: super::time::now_epoch_z(),
        actor: binding.task.assignee.clone(),
        candidate_sha: binding.candidate_sha.clone(),
        task_binding: binding.task.clone(),
        validation_epoch: binding.epoch.clone(),
        proof: proof.clone(),
        record_digest: String::new(),
    };
    record.record_digest = record.calculated_digest()?;
    record.verify()?;
    let event = serde_json::to_value(&record).map_err(invalid)?;
    DbBroker::new(&store.root).with_transaction(
        &events::canonical_db_path(&store.root),
        &record.actor,
        Some(&record.task_binding.task_id),
        "validation.history.complete",
        |conn| {
            todo::check_validation_binding_on_conn(conn, &binding.task, false)?;
            events::ensure_tables(conn)?;
            events::check_subject_append_capacity(
                conn,
                events::VERIFICATION,
                "commit",
                &record.candidate_sha,
                &event,
                MAX_RECORDS,
            )?;
            events::append_on_conn(conn, events::VERIFICATION, &event)?;
            Ok(())
        },
    )
}

/// Typed read of a canonical producer record. No public deserializer or import
/// constructor: history consumption always returns through the bounded journal.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoricalValidationProof {
    #[serde(flatten)]
    record: CompletionRecord,
    #[serde(skip)]
    store_root: PathBuf,
}
impl HistoricalValidationProof {
    pub(crate) fn task_binding(&self) -> &HistoricalTaskBinding {
        &self.record.task_binding
    }
    pub(crate) fn candidate_sha(&self) -> &str {
        &self.record.candidate_sha
    }
    pub(crate) fn accepted_base(&self) -> &str {
        &self.record.proof.material.accepted_base_revision
    }
    pub(crate) fn record_id(&self) -> &str {
        &self.record.event_id
    }
    pub(crate) fn record_digest(&self) -> &str {
        &self.record.record_digest
    }
    pub(crate) fn validation_epoch(&self) -> &ValidationEpochMetadata {
        &self.record.validation_epoch
    }
    pub(crate) fn check_integration_witness(
        &self,
        root: &Path,
        witness: &IntegrationWitness,
    ) -> Result<(), DecapodError> {
        let material = &self.record.proof.material;
        if witness.candidate_sha != self.candidate_sha()
            || witness.accepted_base != self.accepted_base()
            || witness.repository != material.repository
            || witness.base_branch != material.base_branch
        {
            return Err(invalid(
                "integration witness is bound to different proof material or accepted branch",
            ));
        }
        witness.recheck_local(root)
    }
    pub(crate) fn recheck_on_conn(&self, conn: &Connection) -> Result<(), DecapodError> {
        let event = events::query_subject_event_on_conn(
            conn,
            events::VERIFICATION,
            "commit",
            self.candidate_sha(),
            self.record_id(),
        )?
        .ok_or_else(|| invalid("original canonical completion record is missing"))?;
        let record = decode(&event)?;
        if record.record_digest != self.record.record_digest {
            return Err(invalid("original canonical completion record changed"));
        }
        Ok(())
    }
}
fn decode(event: &events::StoredEvent) -> Result<CompletionRecord, DecapodError> {
    let record: CompletionRecord =
        serde_json::from_value(event.payload.clone()).map_err(invalid)?;
    record.verify()?;
    if event.stream != events::VERIFICATION
        || event.event_id != record.event_id
        || event.event_type != record.event_type
        || event.ts != record.ts
        || event.actor != record.actor
    {
        return Err(invalid(
            "canonical event envelope disagrees with its completion record",
        ));
    }
    Ok(record)
}

fn lookup_inner(
    root: &Path,
    store: &Store,
    sha: &str,
    id: Option<&str>,
) -> Result<HistoricalValidationProof, DecapodError> {
    require_sha(sha)?;
    require_local(store, root)?;
    let path = events::canonical_db_path(&store.root);
    if !path.is_file() {
        return Err(invalid(
            "no canonical full validation proof was recorded for this commit",
        ));
    }
    let repository = repository_identity(root)?.canonical_name;
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    let event = if let Some(id) = id {
        events::query_subject_event_on_conn(&conn, events::VERIFICATION, "commit", sha, id)?
    } else {
        let history = events::query_subject_on_conn(&conn, events::VERIFICATION, "commit", sha, MAX_RECORDS)?;
        let mut binding: Option<HistoricalTaskBinding> = None;
        for event in &history {
            let record = decode(event)?;
            if record.candidate_sha != sha || record.proof.material.repository != repository {
                return Err(invalid("subject journal contains another commit or repository"));
            }
            if let Some(previous) = &binding {
                let next = &record.task_binding;
                if previous.task_id != next.task_id || previous.contract_digest != next.contract_digest
                    || previous.assignee != next.assignee || previous.lease_generation != next.lease_generation
                { return Err(invalid("commit has successful records for different task contracts or custody; select the original record explicitly")); }
            }
            binding = Some(record.task_binding);
        }
        history.into_iter().last()
    }.ok_or_else(|| invalid("no canonical full validation proof was recorded for this commit"))?;
    let record = decode(&event)?;
    if record.candidate_sha != sha || record.proof.material.repository != repository {
        return Err(invalid(
            "historical proof belongs to a different commit or repository",
        ));
    }
    Ok(HistoricalValidationProof {
        record,
        store_root: store.root.clone(),
    })
}
pub(crate) fn lookup(
    root: &Path,
    store: &Store,
    sha: &str,
) -> Result<HistoricalValidationProof, DecapodError> {
    lookup_inner(root, store, sha, None)
}
pub(crate) fn lookup_record(
    root: &Path,
    store: &Store,
    sha: &str,
    record_id: &str,
) -> Result<HistoricalValidationProof, DecapodError> {
    lookup_inner(root, store, sha, Some(record_id))
}

/// The accepted integration is distinct from the source candidate that ran the
/// suite. This witness never claims that accepted_sha itself was tested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntegrationWitness {
    pub candidate_sha: String,
    pub accepted_sha: String,
    pub integration_sha: String,
    pub accepted_base: String,
    pub mode: String,
    pub repository: String,
    pub base_branch: String,
}
impl HistoricalValidationProof {
    /// Remote observation belongs before the task transaction. Authorization
    /// may consult the store; transaction-time rechecks only read Git objects.
    pub(crate) fn verify_integration(
        &self,
        root: &Path,
    ) -> Result<IntegrationWitness, DecapodError> {
        let repository = repository_identity(root)?;
        let material = &self.record.proof.material;
        if repository.canonical_name != material.repository {
            return Err(invalid("integration repository differs from proof"));
        }
        let branch_ref = format!("refs/heads/{}", material.base_branch);
        if !git_output(root, &["check-ref-format", &branch_ref])?
            .status
            .success()
        {
            return Err(invalid("accepted branch is not a valid Git ref"));
        }
        let program = crate::readiness_installed_tool(root, "git")?;
        external_action::authorize(
            &self.store_root,
            ExternalCapability::VcsRead,
            "validation.history.integration",
            &program.to_string_lossy(),
        )?;
        let remote = accepted_target(&program, &repository.remote_url, &branch_ref)?;
        let lines: Vec<_> = remote.lines().collect();
        let (accepted_sha, returned_ref) = lines
            .first()
            .and_then(|line| line.split_once('\t'))
            .ok_or_else(|| invalid("accepted target lookup returned no exact branch"))?;
        require_sha(accepted_sha)?;
        if lines.len() != 1 || returned_ref != branch_ref {
            return Err(invalid("accepted target lookup was ambiguous"));
        }
        // Observation is read-only. Fetching is an explicit governed VCS action,
        // never an implicit run of repository-configured transport helpers.
        if !git_output(
            root,
            &["cat-file", "-e", &format!("{accepted_sha}^{{commit}}")],
        )?
        .status
        .success()
        {
            return Err(invalid(
                "accepted target objects are missing; fetch the accepted branch through the governed VCS path, then retry",
            ));
        }
        integration_for_target(root, material, accepted_sha)
    }
}

fn integration_for_target(
    root: &Path,
    material: &MaterialIdentity,
    accepted_sha: &str,
) -> Result<IntegrationWitness, DecapodError> {
    require_sha(accepted_sha)?;
    require_sha(&material.head_revision)?;
    require_sha(&material.accepted_base_revision)?;
    reject_history_overrides(root)?;
    if !ancestor(root, &material.accepted_base_revision, accepted_sha)? {
        return Err(invalid(
            "accepted history does not descend from the proof's accepted base",
        ));
    }
    let (mode, integration_sha) = if ancestor(root, &material.head_revision, accepted_sha)? {
        ("ancestor", material.head_revision.clone())
    } else {
        // Squash/rebase receipt reuse is conservative: an actual accepted
        // commit must contain the complete candidate tree. Patch similarity,
        // PR labels and a green provider wrapper do not establish equivalence.
        let candidate_tree = tree(root, &material.head_revision)?;
        let range = format!("{}..{accepted_sha}", material.accepted_base_revision);
        let commits = git_text(
            root,
            &[
                "log",
                "--first-parent",
                "--max-count=257",
                "--format=%H %T",
                &range,
            ],
        )?;
        let mut found = None;
        let mut count = 0;
        for row in commits.lines() {
            count += 1;
            if count > 256 {
                return Err(invalid(
                    "exact-tree integration was not found within the supported 256 accepted commits; historical completion cannot establish this squash/rebase relation",
                ));
            }
            let (commit, commit_tree) = row
                .split_once(' ')
                .ok_or_else(|| invalid("invalid accepted commit/tree observation"))?;
            require_sha(commit)?;
            require_sha(commit_tree)?;
            if commit_tree == candidate_tree {
                found = Some(commit.to_owned());
                break;
            }
        }
        ("equivalent_tree", found.ok_or_else(|| invalid("candidate is not in accepted history and no exact accepted candidate tree was found"))?)
    };
    let witness = IntegrationWitness {
        candidate_sha: material.head_revision.clone(),
        accepted_sha: accepted_sha.into(),
        integration_sha,
        accepted_base: material.accepted_base_revision.clone(),
        mode: mode.into(),
        repository: material.repository.clone(),
        base_branch: material.base_branch.clone(),
    };
    witness.recheck_local(root)?;
    Ok(witness)
}
impl IntegrationWitness {
    pub(crate) fn recheck_local(&self, root: &Path) -> Result<(), DecapodError> {
        for sha in [
            &self.candidate_sha,
            &self.accepted_sha,
            &self.integration_sha,
            &self.accepted_base,
        ] {
            require_sha(sha)?;
        }
        if !git_output(
            root,
            &[
                "check-ref-format",
                &format!("refs/heads/{}", self.base_branch),
            ],
        )?
        .status
        .success()
        {
            return Err(invalid(
                "integration witness contains an invalid accepted branch",
            ));
        }
        reject_history_overrides(root)?;
        if repository_identity(root)?.canonical_name != self.repository
            || !ancestor(root, &self.accepted_base, &self.accepted_sha)?
            || !ancestor(root, &self.integration_sha, &self.accepted_sha)?
        {
            return Err(invalid(
                "accepted integration witness no longer matches repository or history",
            ));
        }
        match self.mode.as_str() {
            "ancestor"
                if self.integration_sha == self.candidate_sha
                    && ancestor(root, &self.candidate_sha, &self.accepted_sha)? =>
            {
                Ok(())
            }
            "equivalent_tree"
                if ancestor(root, &self.accepted_base, &self.integration_sha)?
                    && tree(root, &self.candidate_sha)? == tree(root, &self.integration_sha)? =>
            {
                Ok(())
            }
            _ => Err(invalid(
                "accepted integration witness is not supported or no longer matches the candidate",
            )),
        }
    }
}
fn git_output(root: &Path, args: &[&str]) -> Result<Output, DecapodError> {
    let program = crate::readiness_installed_tool(root, "git")?;
    let capture = Command::new(program)
        .args([
            "--no-optional-locks",
            "--no-replace-objects",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
            "-c",
            "submodule.recurse=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ])
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_GRAFT_FILE", "/dev/null")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .bounded_capture(
            ProcessLimits {
                timeout: Duration::from_secs(30),
                max_output_bytes: 1024 * 1024,
            },
            &Cancellation::default(),
        )?;
    if capture.termination != Termination::Exited {
        return Err(invalid("bounded Git observation did not finish"));
    }
    Ok(capture.output)
}
fn accepted_target(program: &Path, remote: &str, branch_ref: &str) -> Result<String, DecapodError> {
    // No checkout configuration, URL rewriting, credential helpers, hooks or
    // inherited Git environment can influence this exact public HTTPS read.
    let scratch = tempfile::tempdir()?;
    let mut command = Command::new(program);
    command
        .current_dir(scratch.path())
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CEILING_DIRECTORIES", scratch.path())
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "http.followRedirects=false",
            "ls-remote",
            "--refs",
            "--exit-code",
            remote,
            branch_ref,
        ]);
    for name in [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let capture = command.bounded_capture(
        ProcessLimits {
            timeout: Duration::from_secs(30),
            max_output_bytes: 4096,
        },
        &Cancellation::default(),
    )?;
    if capture.termination != Termination::Exited || !capture.output.status.success() {
        return Err(invalid(
            "accepted target is unavailable through the reviewed public HTTPS read; private-repository historical integration is not supported by this transport",
        ));
    }
    Ok(String::from_utf8(capture.output.stdout)
        .map_err(invalid)?
        .trim()
        .to_owned())
}
fn reject_history_overrides(root: &Path) -> Result<(), DecapodError> {
    // Git ancestry must describe object parents, never a mutable graft file.
    // Alternates can disappear after the transaction and cannot own this proof.
    // Resolve their common directory rather than --git-path: the isolated
    // reader deliberately redirects GIT_GRAFT_FILE to /dev/null. Both physical
    // metadata files belong to the common directory, including in worktrees.
    let output = git_output(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    if !output.status.success() {
        return Err(invalid("Git common directory observation failed"));
    }
    let raw = String::from_utf8(output.stdout).map_err(invalid)?;
    // Directory names may end in whitespace; trim only Git's terminal LF.
    let common_dir = Path::new(
        raw.strip_suffix('\n')
            .ok_or_else(|| invalid("Git common directory output is incomplete"))?,
    );
    if !common_dir.is_absolute() {
        return Err(invalid("Git common directory is not an absolute path"));
    }
    for name in ["info/grafts", "objects/info/alternates"] {
        match std::fs::symlink_metadata(common_dir.join(name)) {
            Ok(_) => {
                return Err(invalid(
                    "historical integration does not support Git grafts or alternate object stores",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn git_text(root: &Path, args: &[&str]) -> Result<String, DecapodError> {
    let output = git_output(root, args)?;
    if !output.status.success() {
        return Err(invalid(format!(
            "Git {} observation failed; ensure accepted commit objects are available",
            args.first().unwrap_or(&"read")
        )));
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(invalid)?
        .trim()
        .to_owned())
}
fn ancestor(root: &Path, before: &str, after: &str) -> Result<bool, DecapodError> {
    let output = git_output(root, &["merge-base", "--is-ancestor", before, after])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(invalid("accepted history is missing or unreadable")),
    }
}
fn tree(root: &Path, commit: &str) -> Result<String, DecapodError> {
    let value = git_text(
        root,
        &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )?;
    require_sha(&value)?;
    Ok(value)
}

/// Real complete-suite fixtures for task transaction tests. This helper exists
/// only in test builds and exercises the same record verifier and event store.
#[cfg(test)]
pub(crate) mod test_fixture {
    use super::*;

    pub(crate) fn persist(
        conn: &Connection,
        store_root: &Path,
        material: MaterialIdentity,
        task: HistoricalTaskBinding,
        epoch: ValidationEpochMetadata,
    ) -> Result<HistoricalValidationProof, DecapodError> {
        let mut record = record();
        record.event_id = super::super::ulid::new_ulid();
        record.proof.invocation_id = record.event_id.clone();
        record.candidate_sha = material.head_revision.clone();
        record.proof.material = material.clone();
        record.proof.suite.before = material.clone();
        record.proof.suite.after = material.clone();
        record.proof.suite.facts.target_branch = material.base_branch.clone();
        for result in &mut record.proof.suite.results {
            result.identity = material.clone();
        }
        record.proof.laya.binding.material = material;
        record.proof.laya.binding.evidence_digest = digest(&record.proof.suite)?;
        record.proof.laya.request_id = digest(&record.proof.laya.binding)?;
        record.actor = task.assignee.clone();
        record.task_binding = task;
        record.validation_epoch = epoch;
        record.record_digest = record.calculated_digest()?;
        record.verify()?;
        events::ensure_tables(conn)?;
        events::append_on_conn(
            conn,
            events::VERIFICATION,
            &serde_json::to_value(&record).map_err(invalid)?,
        )?;
        Ok(HistoricalValidationProof {
            record,
            store_root: store_root.into(),
        })
    }
    pub(crate) fn completed() -> CompletedReadinessEvidence {
        record().proof
    }
    pub(super) fn record() -> CompletionRecord {
        use evidence::*;
        let hash = |label: &str| evidence::digest(&label).unwrap();
        let material = MaterialIdentity {
            repository: "DecapodLabs/decapod".into(),
            work_branch: "work/readiness".into(),
            head_revision: "a".repeat(40),
            base_branch: "master".into(),
            accepted_base_revision: "b".repeat(40),
            history_digest: hash("history"),
            tree_digest: hash("tree"),
            configuration_digest: hash("config"),
            workflow_digest: hash("workflow"),
            toolchain_digest: hash("tools"),
            candidate_digest: hash("candidate"),
        };
        let checks = [
            CheckKind::SourceCi,
            CheckKind::DerivedRelease,
            CheckKind::CandidateBuild,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| ExpectedCheck {
            key: CheckKey {
                provider: Provider::GithubActions,
                workflow: ".github/workflows/ci.yml".into(),
                job: format!("check-{index}"),
                matrix: Default::default(),
                profile: "linux-x86_64-debug".into(),
            },
            kind,
            definition_digest: hash(&format!("definition-{index}")),
            invocation_digest: hash(&format!("invocation-{index}")),
            toolchain_digest: hash("tools"),
            executor: Provider::Native,
            dependencies: vec![],
            requirement: Requirement::Execute,
        })
        .collect();
        let plan = ExpectedPlan {
            schema_version: SCHEMA_VERSION.into(),
            policy_digest: hash("policy"),
            source_inventory_digest: hash("inventory"),
            buildkite_enabled: false,
            checks,
        };
        let suite = SuiteEvidence {
            schema_version: SCHEMA_VERSION.into(),
            expected_plan_digest: plan.digest().unwrap(),
            before: material.clone(),
            after: material.clone(),
            facts: ApplicabilityFacts {
                event: "pull_request".into(),
                target_branch: "master".into(),
                changed_paths: ["src/lib.rs".into()].into(),
            },
            results: plan
                .checks
                .iter()
                .map(|check| CheckResult {
                    key: check.key.clone(),
                    definition_digest: check.definition_digest.clone(),
                    invocation_digest: check.invocation_digest.clone(),
                    toolchain_digest: check.toolchain_digest.clone(),
                    executor: Provider::Native,
                    identity: material.clone(),
                    observation: Observation::Executed {
                        execution_id: format!("run-{}", check.key.job),
                        conclusion: Conclusion::Success,
                        exit_code: Some(0),
                        output_digest: hash("output"),
                        artifact_digest: (check.kind == CheckKind::CandidateBuild)
                            .then(|| hash("built artifact")),
                    },
                })
                .collect(),
        };
        let binding = laya_readiness::Binding {
            material: material.clone(),
            governance_digest: hash("governance"),
            proof_plan_digest: plan.digest().unwrap(),
            evidence_digest: evidence::digest(&suite).unwrap(),
            configuration_digest: hash("laya configuration"),
            trajectory_run_id: "run-trajectory".into(),
        };
        let laya = laya_readiness::Receipt {
            schema: "decapod.laya-readiness.v1".into(),
            decapod_version: env!("CARGO_PKG_VERSION").into(),
            request_id: evidence::digest(&binding).unwrap(),
            binding,
            rubric: "test".into(),
            input_digest: hash("input"),
            input_bytes: 0,
            mode: laya_readiness::Mode::Disabled,
            decision: laya_readiness::Decision::Disabled,
            reason: "disabled".into(),
            judgment: None,
            manifest: None,
            attempts: vec![],
            runtime_sha256: None,
            capability: None,
            cpu_fallback_reason: None,
            accuracy_scope: "test fixture".into(),
        };
        let proof = CompletedReadinessEvidence::from_runtime(
            "runtime-invocation",
            validation_runtime::READINESS_SCOPE,
            &material,
            &hash("governance"),
            &hash("trajectory"),
            &plan,
            &suite,
            &laya,
        )
        .unwrap();
        let epoch = ValidationEpochMetadata {
            schema_version: validation_epoch::VALIDATION_EPOCH_SCHEMA_VERSION.into(),
            epoch_id: "ve_test".into(),
            evaluator_identity: "test".into(),
            evaluator_set_hash: hash("evaluators"),
            constitution_version: "test".into(),
            constitution_hash: hash("constitution"),
            validation_profile: "test".into(),
            validation_profile_hash: hash("profile"),
            proof_rubric: "test".into(),
            proof_rubric_hash: hash("rubric"),
            generated_specs_manifest_hash: hash("manifest"),
            generated_specs_fingerprint: hash("specs"),
            material_hashes: Default::default(),
        };
        let mut record = CompletionRecord {
            schema: SCHEMA.into(),
            result: "success".into(),
            event_id: "runtime-invocation".into(),
            event_type: EVENT_TYPE.into(),
            ts: "2026-11-11T00:00:00Z".into(),
            actor: "agent-a".into(),
            candidate_sha: material.head_revision,
            task_binding: HistoricalTaskBinding {
                task_id: "task-a".into(),
                revision: 7,
                status: "doing".into(),
                contract_digest: hash("contract"),
                checkpoint_digest: hash("checkpoint"),
                assignee: "agent-a".into(),
                lease_generation: 2,
            },
            validation_epoch: epoch,
            proof,
            record_digest: String::new(),
        };
        record.record_digest = record.calculated_digest().unwrap();
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_fixture::record;

    fn reseal(record: &mut CompletionRecord) {
        record.proof.laya.binding.evidence_digest = digest(&record.proof.suite).unwrap();
        record.proof.laya.request_id = digest(&record.proof.laya.binding).unwrap();
        record.record_digest = record.calculated_digest().unwrap();
    }

    #[test]
    fn terminal_proof_rejects_partial_forged_and_current_binding_mismatches() {
        let original = record().proof;
        assert!(
            original
                .verify_current(
                    &original.material,
                    &original.governance_digest,
                    &original.trajectory_digest
                )
                .is_ok()
        );
        for field in ["source", "accepted_target", "governance", "trajectory"] {
            let mut material = original.material.clone();
            let mut governance = original.governance_digest.clone();
            let mut trajectory = original.trajectory_digest.clone();
            match field {
                "source" => material.tree_digest = digest(&"different source").unwrap(),
                "accepted_target" => material.accepted_base_revision = "c".repeat(40),
                "governance" => governance = digest(&"different governance").unwrap(),
                _ => trajectory = digest(&"different trajectory").unwrap(),
            }
            assert!(
                original
                    .verify_current(&material, &governance, &trajectory)
                    .is_err(),
                "{field}"
            );
        }
        for defect in ["empty", "partial", "failed", "unknown", "forged_output"] {
            let mut proof = original.clone();
            match defect {
                "empty" => proof.suite.results.clear(),
                "partial" => {
                    proof.suite.results.pop();
                }
                "failed" => {
                    proof.suite.results[0].observation = evidence::Observation::Executed {
                        execution_id: "failed".into(),
                        conclusion: evidence::Conclusion::Failure,
                        exit_code: Some(1),
                        output_digest: digest(&"failure").unwrap(),
                        artifact_digest: None,
                    }
                }
                "unknown" => proof.schema = "unknown".into(),
                _ => proof.suite.results[0].identity.head_revision = "d".repeat(40),
            }
            assert!(
                proof
                    .verify_current(
                        &proof.material,
                        &proof.governance_digest,
                        &proof.trajectory_digest
                    )
                    .is_err(),
                "{defect}"
            );
        }
    }

    #[test]
    fn runner_hardening_is_inert_but_active_git_overrides_are_rejected() {
        for (name, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_COUNT", "0"),
        ] {
            assert!(
                !ambient_git_override(OsStr::new(name), OsStr::new(value)),
                "reviewed runner hardening rejected: {name}={value}"
            );
        }
        for (name, value) in [
            ("GIT_CONFIG_NOSYSTEM", "0"),
            ("GIT_CONFIG_NOSYSTEM", ""),
            ("GIT_CONFIG_GLOBAL", "/tmp/injected-config"),
            ("GIT_CONFIG_GLOBAL", "/dev/null "),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_COUNT", "00"),
            ("GIT_CONFIG", ""),
            (
                "GIT_CONFIG_PARAMETERS",
                "'remote.origin.url=https://example.invalid'",
            ),
            ("GIT_CONFIG_KEY_0", "remote.origin.url"),
            ("GIT_CONFIG_VALUE_0", "https://example.invalid"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_DIR", "/tmp/other-repository"),
            ("GIT_WORK_TREE", "/tmp/other-worktree"),
            ("GIT_COMMON_DIR", "/tmp/other-common-dir"),
            ("GIT_INDEX_FILE", "/tmp/other-index"),
            ("GIT_OBJECT_DIRECTORY", "/tmp/other-objects"),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", "/tmp/other-objects"),
            ("GIT_GRAFT_FILE", "/dev/null"),
            ("GIT_REPLACE_REF_BASE", "refs/other-replacements"),
            ("GIT_NAMESPACE", "other-namespace"),
            ("GIT_EXEC_PATH", "/tmp/other-git-tools"),
            ("GIT_CEILING_DIRECTORIES", "/tmp/other-discovery-root"),
            ("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1"),
        ] {
            assert!(
                ambient_git_override(OsStr::new(name), OsStr::new(value)),
                "active or unsupported Git override accepted: {name}={value}"
            );
        }
    }

    #[test]
    fn complete_native_proof_roundtrips_and_commits_to_all_material() {
        let proof = record();
        proof.verify().unwrap();
        let mut changed: CompletionRecord =
            serde_json::from_slice(&serde_json::to_vec(&proof).unwrap()).unwrap();
        changed.verify().unwrap();
        changed.task_binding.contract_digest = digest(&"other intent").unwrap();
        assert!(changed.verify().is_err());
    }

    #[test]
    fn failed_missing_reused_and_stale_evidence_cannot_be_resealed_as_success() {
        for mutation in 0..4 {
            let mut proof = record();
            match mutation {
                0 => {
                    proof.proof.suite.results.pop();
                }
                1 => {
                    if let evidence::Observation::Executed {
                        conclusion,
                        exit_code,
                        ..
                    } = &mut proof.proof.suite.results[0].observation
                    {
                        *conclusion = evidence::Conclusion::Failure;
                        *exit_code = Some(1);
                    }
                }
                2 => {
                    proof.proof.suite.results[0].observation = evidence::Observation::Reused {
                        evidence_digest: digest(&"old success").unwrap(),
                    };
                }
                _ => {
                    proof.proof.suite.after.head_revision = "c".repeat(40);
                }
            }
            reseal(&mut proof);
            assert!(proof.verify().is_err(), "mutation {mutation} was accepted");
        }
    }

    #[test]
    fn ci_local_partial_success_and_blocking_laya_are_not_full_readiness() {
        let mut proof = record();
        proof.proof.scope = "ci_local".into();
        reseal(&mut proof);
        assert!(proof.verify().is_err());
        let mut proof = record();
        proof.proof.laya.decision = laya_readiness::Decision::Abstain;
        reseal(&mut proof);
        assert!(proof.verify().is_err());
        let mut proof = record();
        proof.proof.laya.binding.material.head_revision = "c".repeat(40);
        reseal(&mut proof);
        assert!(proof.verify().is_err());
    }

    #[test]
    fn required_laya_retains_the_real_attempt_request_id_and_rejects_response_drift() {
        use super::super::laya_protocol as wire;
        let mut proof = record();
        let receipt = &mut proof.proof.laya;
        receipt.mode = laya_readiness::Mode::Required;
        receipt.decision = laya_readiness::Decision::Accept;
        receipt.rubric = wire::RUBRIC.into();
        receipt.input_bytes = 100;
        let manifest = super::super::laya_assets::baseline();
        let manifest_digest = super::super::laya_assets::manifest_digest(&manifest).unwrap();
        let runtime_hash = digest(&"frozen runtime executable").unwrap();
        let installation = laya_readiness::Installation {
            directory: PathBuf::from("/trusted/model"),
            manifest_digest: manifest_digest.clone(),
            runtime: PathBuf::from("/trusted/runtime"),
            runtime_sha256: runtime_hash.clone(),
            cpu_runtime: None,
        };
        receipt.request_id = digest(&(
            &receipt.binding,
            wire::RUBRIC,
            &receipt.input_digest,
            &installation,
            wire::Backend::Cpu,
            &runtime_hash,
            0,
        ))
        .unwrap();
        assert_ne!(receipt.request_id, digest(&receipt.binding).unwrap());
        receipt.runtime_sha256 = Some(runtime_hash);
        receipt.manifest = Some(manifest);
        let judgment = wire::Judgment {
            true_probability: 0.8,
            action_probability: 0.9,
            probability_quantum: 1e-4,
            input_tokens: 20,
            backend: wire::Backend::Cpu,
            runtime: "onnxruntime-cpu".into(),
            inference_ms: 2.0,
        };
        receipt.judgment = Some(judgment.clone());
        receipt.attempts = vec![wire::Response {
            schema: wire::WORKER_SCHEMA.into(),
            rubric: wire::RUBRIC.into(),
            request_id: receipt.request_id.clone(),
            input_digest: receipt.input_digest.clone(),
            manifest_digest,
            publication_ready: false,
            judgment: Some(judgment),
            error_kind: None,
            error: None,
            metrics: None,
        }];
        proof.record_digest = proof.calculated_digest().unwrap();
        proof.verify().unwrap();
        proof.proof.laya.attempts[0].request_id = digest(&"another validation").unwrap();
        proof.record_digest = proof.calculated_digest().unwrap();
        assert!(proof.verify().is_err());
    }

    #[test]
    fn canonical_event_keeps_complete_proof_and_detects_envelope_or_payload_changes() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("decapod.db")).unwrap();
        events::ensure_tables(&conn).unwrap();
        let record = record();
        let event = serde_json::to_value(&record).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        events::check_subject_append_capacity(
            &conn,
            events::VERIFICATION,
            "commit",
            &record.candidate_sha,
            &event,
            MAX_RECORDS,
        )
        .unwrap();
        events::append_on_conn(&conn, events::VERIFICATION, &event).unwrap();
        conn.execute_batch("COMMIT").unwrap();
        let mut stored = events::query_subject_event_on_conn(
            &conn,
            events::VERIFICATION,
            "commit",
            &record.candidate_sha,
            &record.event_id,
        )
        .unwrap()
        .unwrap();
        let decoded = decode(&stored).unwrap();
        assert_eq!(decoded.proof.expected_plan, record.proof.expected_plan);
        assert_eq!(decoded.proof.suite, record.proof.suite);
        // No runtime evidence path or surviving temporary directory is needed.
        let proof = HistoricalValidationProof {
            record,
            store_root: dir.path().into(),
        };
        proof.recheck_on_conn(&conn).unwrap();
        stored.actor = "other-agent".into();
        assert!(decode(&stored).is_err());
        conn.execute(
            "UPDATE events SET payload = '{}' WHERE stream = ?1 AND event_id = ?2",
            db::params![events::VERIFICATION, proof.record_id()],
        )
        .unwrap();
        assert!(proof.recheck_on_conn(&conn).is_err());
    }

    #[test]
    fn historical_lookup_is_read_only_and_rejects_symbolic_or_abbreviated_refs() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            kind: super::super::store::StoreKind::Repo,
            root: dir.path().join("absent"),
        };
        for sha in ["HEAD", "abc123", "--all", &"A".repeat(40)] {
            assert!(lookup(dir.path(), &store, sha).is_err());
        }
        assert!(lookup(dir.path(), &store, &"a".repeat(40)).is_err());
        assert!(!store.root.exists());
    }

    #[test]
    fn outer_archive_failure_revokes_publication_and_retains_execution_evidence() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".decapod")).unwrap();
        std::fs::write(
            dir.path().join(".decapod/config.toml"),
            toml::to_string(&crate::cli::DecapodProjectConfig::default()).unwrap(),
        )
        .unwrap();
        let record = record();
        let path = dir.path().join("missing-runtime-evidence");
        let store = Store {
            kind: super::super::store::StoreKind::Repo,
            root: dir.path().join("store"),
        };
        let binding = ValidationBinding {
            repository: record.proof.material.repository.clone(),
            candidate_sha: record.candidate_sha.clone(),
            governance_digest: record.proof.governance_digest.clone(),
            trajectory_digest: record.proof.trajectory_digest.clone(),
            trajectory_run_id: "run-trajectory".into(),
            task: record.task_binding.clone(),
            epoch: record.validation_epoch.clone(),
        };
        let mut report = ValidationReport {
            ready: true,
            proof_ready: true,
            laya_readiness: Some(record.proof.laya.clone()),
            scope: validation_runtime::READINESS_SCOPE,
            agent_guidance: validation_runtime::ValidationGuidance::for_outcome(true),
            stage_dispositions: vec![],
            blockers: vec![],
            evidence_path: Some(path.clone()),
            completed_evidence: Some(record.proof),
        };
        crate::archive_workbranch_readiness(&store, dir.path(), Some(&binding), &mut report);
        assert!(!report.ready);
        assert!(report.proof_ready);
        assert_eq!(report.evidence_path.as_ref(), Some(&path));
        assert_eq!(
            report.agent_guidance.next_action,
            validation_runtime::ValidationGuidance::for_outcome(false).next_action
        );
        assert!(
            report
                .blockers
                .iter()
                .any(|blocker| blocker.code == "VALIDATION_HISTORY_PERSIST_FAILED")
        );
        assert!(!store.root.exists());
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/validation_history_integration_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "../../../tests/unit/core/validation_history_archive_tests.rs"]
mod archive_tests;
