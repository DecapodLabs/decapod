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
    native_proof::{self, IdentityInputs, SuiteLimits, ToolchainInputs},
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
    collections::BTreeMap,
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

fn repository_identity(root: &Path) -> Result<workspace::ReadinessRepository, DecapodError> {
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

/// Bounded references needed to remeasure a workunit observation. No command,
/// source snapshot, output blob, credential or new evidence store is introduced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CurrentEvidenceInputs {
    schema: String,
    inputs: IdentityInputs,
    native_bindings: Vec<(evidence::CheckKey, String)>,
    toolchains: BTreeMap<String, ToolchainInputs>,
    remote_bindings: Option<Vec<(evidence::CheckKey, String)>>,
}
impl CurrentEvidenceInputs {
    pub(super) fn capture(
        inputs: &IdentityInputs,
        selection: &validation_runtime::ExecutionSelection,
    ) -> Result<Self, DecapodError> {
        let mut toolchains = BTreeMap::new();
        let mut native_bindings = Vec::new();
        for (key, invocation) in &selection.native_invocations {
            let tools = ToolchainInputs::from(invocation);
            let reference = digest(&tools)?;
            toolchains.entry(reference.clone()).or_insert(tools);
            native_bindings.push((key.clone(), reference));
        }
        Ok(Self {
            schema: "decapod.current-evidence-inputs.v1".into(),
            inputs: inputs.clone(),
            native_bindings,
            toolchains,
            remote_bindings: selection.policy.buildkite_preflights.then(|| {
                selection
                    .bindings
                    .iter()
                    .filter(|(_, binding)| binding.executor != evidence::Provider::Native)
                    .map(|(key, binding)| (key.clone(), binding.toolchain_digest.clone()))
                    .collect()
            }),
        })
    }
    fn verify_current(&self, root: &Path, expected: &MaterialIdentity) -> Result<(), DecapodError> {
        if self.schema != "decapod.current-evidence-inputs.v1"
            || self.inputs.root != root.canonicalize()?
            || self.inputs.git_program != crate::readiness_installed_tool(root, "git")?
            || self.inputs.repository != expected.repository
            || self.inputs.base_branch != expected.base_branch
            || self.native_bindings.len() > 1024
            || self.toolchains.len() > 1024
            || self
                .remote_bindings
                .as_ref()
                .is_some_and(|bindings| bindings.len() > 1024)
        {
            return Err(invalid(
                "current proof input identity or bounds are invalid",
            ));
        }
        if expected.repository.starts_with("local-git:") {
            let repository = repository_identity(root)?;
            let target = repository
                .local_target_root
                .ok_or_else(|| invalid("local target authority changed"))?;
            let reference = format!("refs/heads/{}^{{commit}}", expected.base_branch);
            let current = workspace::read_canonical_local_git(
                &target,
                &self.inputs.git_program,
                &["rev-parse", "--verify", &reference],
            )?;
            if repository.canonical_name != expected.repository
                || current != expected.accepted_base_revision
            {
                return Err(invalid(
                    "WORKUNIT_PROOF_STALE: canonical local accepted target changed; revalidate the resulting integration",
                ));
            }
        }
        for (reference, tools) in &self.toolchains {
            if digest(tools)? != *reference {
                return Err(invalid("canonical tool references changed"));
            }
        }
        let tools = self
            .native_bindings
            .iter()
            .map(|(key, reference)| {
                let tools = self
                    .toolchains
                    .get(reference)
                    .ok_or_else(|| invalid("canonical tool references are incomplete"))?;
                Ok((key.clone(), tools.clone()))
            })
            .collect::<Result<Vec<_>, DecapodError>>()?;
        let mut current = native_proof::capture_readonly_identity(
            &self.inputs,
            &tools,
            SuiteLimits {
                timeout: Duration::from_secs(30),
                max_checks: 1024,
                max_hashed_bytes: 2 * 1024 * 1024 * 1024,
                max_output_bytes: 1024 * 1024,
                git_output_limit: 1024 * 1024,
            },
            &Cancellation::default(),
        )?;
        if let Some(remote) = &self.remote_bindings {
            // Preserve the original declared/attested remote assurance scope;
            // rehashing local references never claims to measure remote tools.
            current.toolchain_digest = digest(&(&current.toolchain_digest, remote))?;
        }
        if &current != expected {
            return Err(invalid(
                "WORKUNIT_PROOF_STALE: source, candidate, accepted target, policy, history or measured tool identity changed; obtain fresh full validation",
            ));
        }
        Ok(())
    }
}

pub(crate) fn ensure_full_validation_plan(gates: &[String]) -> Result<(), DecapodError> {
    if gates.len() != 1 || !matches!(gates[0].as_str(), "validate_passes" | "validated_commit") {
        return Err(DecapodError::ValidationError(
            "VALIDATION_HISTORY_TASK_PROOF_UNSUPPORTED: recorded full validation cannot discharge additional or missing task proof obligations".into(),
        ));
    }
    Ok(())
}

/// Captured before any validation execution, not reconstructed from its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidationBinding {
    repository: String,
    candidate_sha: String,
    governance_digest: String,
    trajectory_digest: String,
    trajectory_run_id: String,
    task: HistoricalTaskBinding,
    epoch: ValidationEpochMetadata,
    workunit_subject: Option<super::workunit::WorkUnitValidationSubject>,
}

/// A live validation result carried to publication, never loaded from a receipt.
/// Retains the existing proof and custody owners across push and forge I/O.
pub(crate) struct PublicationReadiness {
    proof: CompletedReadinessEvidence,
    binding: Option<ValidationBinding>,
    evidence_path: PathBuf,
    snapshot_handoff: Option<PathBuf>,
}

impl PublicationReadiness {
    pub(crate) fn from_report(
        mut report: ValidationReport,
        binding: Option<ValidationBinding>,
    ) -> Result<Self, DecapodError> {
        if !report.publication_ready() || !report.proof_ready || !report.blockers.is_empty() {
            return Err(invalid(
                "PUBLICATION_NOT_READY: complete live proof is required",
            ));
        }
        let proof = report.completed_evidence.take().ok_or_else(|| {
            invalid("PUBLICATION_PROOF_MISSING: a receipt or successful report alone cannot authorize a PR")
        })?;
        if proof.target_scope.is_some() || report.target.is_some() {
            return Err(invalid(
                "PUBLICATION_NOT_READY: target-scoped validation cannot authorize full-repository PR publication",
            ));
        }
        proof.verify()?;
        if proof.scope != report.scope
            || digest(&report.laya_readiness)? != digest(&Some(&proof.laya))?
            || proof.current_inputs.is_none()
            || binding.as_ref().is_some_and(|binding| {
                binding.repository != proof.material.repository
                    || binding.candidate_sha != proof.material.head_revision
                    || binding.governance_digest != proof.governance_digest
                    || binding.trajectory_digest != proof.trajectory_digest
            })
        {
            return Err(invalid(
                "PUBLICATION_PROOF_MISMATCH: live validation subject differs",
            ));
        }
        let evidence_path = report
            .evidence_path
            .ok_or_else(|| invalid("PUBLICATION_PROOF_MISSING: live evidence directory is absent"))?
            .canonicalize()?;
        let snapshot_handoff =
            Some(evidence_path.join("snapshot-handoff.json")).filter(|path| path.is_file());
        Ok(Self {
            proof,
            binding,
            evidence_path,
            snapshot_handoff,
        })
    }

    pub(crate) fn verify_subject(
        &self,
        repository: &str,
        branch: &str,
        head: &str,
        base_branch: &str,
        base: &str,
    ) -> Result<(), DecapodError> {
        let material = &self.proof.material;
        if !material.repository.eq_ignore_ascii_case(repository)
            || material.work_branch != branch
            || material.head_revision != head
            || material.base_branch != base_branch
            || material.accepted_base_revision != base
        {
            return Err(invalid(
                "PUBLICATION_PROOF_SUBJECT_CHANGED: repository, branch, head or accepted base differs from successful validation; reconcile and revalidate",
            ));
        }
        Ok(())
    }

    pub(crate) fn verify_current(&self, root: &Path) -> Result<(), DecapodError> {
        validation_runtime::require_full_readiness_environment()?;
        let (governance, trajectory, _) = governance(root)?;
        self.proof
            .verify_current(&self.proof.material, &governance, &trajectory)?;
        self.proof
            .current_inputs
            .as_ref()
            .ok_or_else(|| invalid("PUBLICATION_PROOF_MISSING: current input witness is absent"))?
            .verify_current(root, &self.proof.material)?;
        self.proof.verify_observed_authority(root)?;
        let laya = laya_readiness::configuration(root)?;
        if laya.mode != self.proof.laya.mode
            || digest(&laya)? != self.proof.laya.binding.configuration_digest
        {
            return Err(invalid(
                "PUBLICATION_LAYA_CHANGED: readiness decision policy changed; revalidate",
            ));
        }
        if laya.mode == laya_readiness::Mode::Required {
            let captured = laya_readiness::capture_installation(&laya);
            let installation = captured
                .as_ref()
                .map_err(invalid)?
                .as_ref()
                .ok_or_else(|| {
                    invalid("PUBLICATION_LAYA_CHANGED: required installation is absent")
                })?;
            let runtime = self.proof.laya.runtime_sha256.as_deref();
            if runtime != Some(installation.runtime_sha256.as_str())
                && runtime
                    != installation
                        .cpu_runtime
                        .as_ref()
                        .map(|cpu| cpu.runtime_sha256.as_str())
            {
                return Err(invalid(
                    "PUBLICATION_LAYA_CHANGED: actual decision runtime differs from successful proof; revalidate",
                ));
            }
            laya_readiness::verify_installation_snapshot(
                &laya,
                &captured,
                std::time::Instant::now() + Duration::from_secs(30),
                &Cancellation::default(),
            )?;
        }
        if let Some(binding) = &self.binding {
            let store = Store {
                kind: super::store::StoreKind::Repo,
                root: super::store::find_governance_root(root).join(".decapod/data"),
            };
            if capture_task_binding(&store, root)?.as_ref() != Some(binding) {
                return Err(invalid(
                    "PUBLICATION_CUSTODY_CHANGED: task, lease, epoch or workunit changed after validation; revalidate",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn snapshot_handoff(&self) -> Option<&Path> {
        self.snapshot_handoff.as_deref()
    }

    pub(crate) fn record_publication_rejection(
        &self,
        reason: &str,
    ) -> Result<PathBuf, DecapodError> {
        // Append a separate rejection; never rewrite successful historical proof.
        let metadata = std::fs::symlink_metadata(&self.evidence_path)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || self.evidence_path.canonicalize()? != self.evidence_path
        {
            return Err(invalid("publication evidence directory changed"));
        }
        let path = self.evidence_path.join(format!(
            "publication-rejection-{}.json",
            super::ulid::new_ulid()
        ));
        let value = serde_json::json!({
            "schema": "decapod.publication-rejection.v1",
            "admitted": false,
            "merge_authorized": false,
            "material": self.proof.material,
            "proof_plan_digest": self.proof.expected_plan.digest().map_err(invalid)?,
            "reason": reason.chars().take(16_384).collect::<String>(),
        });
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(&serde_json::to_vec_pretty(&value).map_err(invalid)?)?;
        file.sync_all()?;
        Ok(path)
    }
}

pub(crate) fn capture_task_binding(
    store: &Store,
    root: &Path,
) -> Result<Option<ValidationBinding>, DecapodError> {
    capture_task_binding_inner(store, root, true)
}

pub(crate) fn capture_task_binding_with_budget(
    store: &Store,
    root: &Path,
    validation_budget: Duration,
) -> Result<Option<ValidationBinding>, DecapodError> {
    if history_is_cloud(store, root)? {
        return Ok(None);
    }
    let (_, _, trajectory) = governance(root)?;
    let task_id = trajectory
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("full validation requires a task-bound trajectory"))?;
    // Never rotate custody implicitly: renewing bumps `lease_generation`, which
    // invalidates workunit execution checkpoints. Default is a read-only advisory.
    let mode = todo::ValidationLeaseTtlMode::from_env_value(
        std::env::var("DECAPOD_VALIDATE_LEASE_TTL").ok().as_deref(),
    );
    let min_ttl_secs = validation_budget.as_secs().saturating_add(300);
    if let todo::ValidationLeaseTtl::Low { advisory, .. } =
        todo::ensure_validation_lease_ttl(&store.root, task_id, min_ttl_secs, mode)?
    {
        eprintln!("{advisory}");
    }
    capture_task_binding_inner(store, root, true)
}
fn capture_task_binding_inner(
    store: &Store,
    root: &Path,
    include_workunit: bool,
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
    let workunit_subject = if include_workunit {
        super::workunit::capture_validation_subject(root, task_id)?
    } else {
        None
    };
    Ok(Some(ValidationBinding {
        repository,
        candidate_sha,
        governance_digest,
        trajectory_digest,
        trajectory_run_id,
        task,
        epoch,
        workunit_subject,
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetProofScope {
    pub(crate) target: String,
    pub(crate) resolved_targets: Vec<String>,
}

/// Owned by ValidationReport and skipped in its public JSON. The constructor is
/// private to the runtime's successful proof path; deserialization alone is not
/// a way to mint a report or append a canonical completion record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedReadinessEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observed_path: Option<super::trajectory::path::Judgment>,
    schema: String,
    decapod_version: String,
    invocation_id: String,
    scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_scope: Option<TargetProofScope>,
    material: MaterialIdentity,
    governance_digest: String,
    trajectory_digest: String,
    expected_plan: ExpectedPlan,
    suite: SuiteEvidence,
    laya: laya_readiness::Receipt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_inputs: Option<CurrentEvidenceInputs>,
}

impl CompletedReadinessEvidence {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_runtime(
        invocation_id: &str,
        scope: &str,
        target_scope: Option<TargetProofScope>,
        material: &MaterialIdentity,
        governance_digest: &str,
        trajectory_digest: &str,
        expected_plan: &ExpectedPlan,
        suite: &SuiteEvidence,
        laya: &laya_readiness::Receipt,
        current_inputs: Option<CurrentEvidenceInputs>,
    ) -> Result<Self, DecapodError> {
        let value = Self {
            observed_path: None,
            schema: SCHEMA.into(),
            decapod_version: env!("CARGO_PKG_VERSION").into(),
            invocation_id: invocation_id.into(),
            scope: scope.into(),
            target_scope,
            material: material.clone(),
            governance_digest: governance_digest.into(),
            trajectory_digest: trajectory_digest.into(),
            expected_plan: expected_plan.clone(),
            suite: suite.clone(),
            laya: laya.clone(),
            current_inputs,
        };
        value.verify()?;
        Ok(value)
    }
    pub(super) fn bind_observed_path(
        mut self,
        observation: &super::trajectory::path::OwnedObservation,
    ) -> Result<Self, DecapodError> {
        observation
            .judgment()
            .verify_retained(&self.material, &self.suite)
            .map_err(invalid)?;
        self.observed_path = Some(observation.judgment().clone());
        self.schema = "decapod.validation-history-proof.v1.1".into();
        self.verify()?;
        Ok(self)
    }
    pub(crate) fn changed_paths(&self) -> &std::collections::BTreeSet<String> {
        &self.suite.facts.changed_paths
    }
    pub(crate) fn verify_observed_authority(&self, root: &Path) -> Result<(), DecapodError> {
        if let Some(path) = &self.observed_path {
            path.verify_current_authority(root).map_err(invalid)?;
        }
        Ok(())
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn target_scope(&self) -> Option<&TargetProofScope> {
        self.target_scope.as_ref()
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
        let is_local_git = self.material.repository.starts_with("local-git:sha256:");
        let scope = match (&self.target_scope, local, is_local_git) {
            (Some(target_scope), true, true) => {
                if target_scope.target.trim().is_empty()
                    || !target_scope.resolved_targets.contains(&target_scope.target)
                {
                    return Err(invalid(
                        "target-scoped readiness lacks valid target closure",
                    ));
                }
                super::repository_profile::LOCAL_GIT_TARGET_READINESS_SCOPE
            }
            (Some(target_scope), true, false) => {
                if target_scope.target.trim().is_empty()
                    || !target_scope.resolved_targets.contains(&target_scope.target)
                {
                    return Err(invalid(
                        "target-scoped readiness lacks valid target closure",
                    ));
                }
                super::repository_profile::TARGET_READINESS_SCOPE
            }
            (Some(_), false, _) => {
                return Err(invalid(
                    "target-scoped readiness requires repository-local checks",
                ));
            }
            (None, true, true) => super::repository_profile::LOCAL_GIT_READINESS_SCOPE,
            (None, true, false) => super::repository_profile::READINESS_SCOPE,
            (None, false, _) => validation_runtime::READINESS_SCOPE,
        };
        if (self.schema != SCHEMA && self.schema != "decapod.validation-history-proof.v1.1")
            || (self.schema == SCHEMA && self.observed_path.is_some())
            || (self.schema == "decapod.validation-history-proof.v1.1"
                && self.observed_path.is_none())
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
        if let Some(path) = &self.observed_path {
            path.verify_retained(&self.material, &self.suite)
                .map_err(invalid)?;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workunit_subject: Option<super::workunit::WorkUnitValidationSubject>,
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
        if self.workunit_subject.is_some() != self.proof.current_inputs.is_some() {
            return Err(invalid(
                "workunit observation and current-input provenance disagree",
            ));
        }
        if let Some(subject) = &self.workunit_subject {
            let sha256 = |value: &str| {
                value.strip_prefix("sha256:").is_some_and(|v| {
                    v.len() == 64
                        && v.bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
            };
            if subject.generation == 0
                || !sha256(&subject.contract_digest)
                || !sha256(&format!("sha256:{}", subject.manifest_digest))
            {
                return Err(invalid(
                    "workunit observation has malformed identity or generation",
                ));
            }
        }
        self.proof.verify()
    }
}

fn history_is_cloud(store: &Store, root: &Path) -> Result<bool, DecapodError> {
    let selected = crate::cli::DecapodProjectConfig::load(root)?
        .repo
        .effective_backend();
    let store_project = super::store::find_decapod_project_root(&store.root)
        .ok()
        .filter(|found| store.root.starts_with(found.join(".decapod")))
        .unwrap_or_else(|| root.into());
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
        if let Some(subject) = &binding.workunit_subject
            && matches!(
                report.scope,
                validation_runtime::READINESS_SCOPE
                    | super::repository_profile::READINESS_SCOPE
                    | super::repository_profile::LOCAL_GIT_READINESS_SCOPE
            )
        {
            let current = capture_task_binding_inner(store, root, false)?
                .ok_or_else(|| invalid("current task authority is unavailable"))?;
            let mut original = binding.clone();
            original.workunit_subject = None;
            if current != original {
                return Err(invalid(
                    "task, custody or validation context changed during validation",
                ));
            }
            super::workunit::invalidate_current_validation(root, &binding.task, subject)?;
        }
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
    if binding.workunit_subject
        != super::workunit::capture_validation_subject(root, &binding.task.task_id)?
    {
        return Err(invalid("workunit changed during full validation"));
    }
    let mut archived = proof.clone();
    archived.verify_observed_authority(root)?;
    if binding.workunit_subject.is_some() {
        archived
            .current_inputs
            .as_ref()
            .ok_or_else(|| invalid("workunit lacks observed current-proof inputs"))?
            .verify_current(root, &archived.material)?;
    } else {
        // Non-workunit history retains its existing bounded representation.
        // A later workunit cannot borrow that observation without fresh proof.
        archived.current_inputs = None;
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
        proof: archived,
        workunit_subject: binding.workunit_subject.clone(),
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
    /// Read-only applicability projection. Historical evidence never becomes a
    /// live PublicationReadiness object, even when its current inputs match.
    pub(crate) fn diagnostic_summary(&self, root: &Path, offline: bool) -> Value {
        let source = &self.record.proof.material;
        let current = || -> Result<(), DecapodError> {
            let (governance, trajectory, _) = governance(root)?;
            self.record
                .proof
                .verify_current(source, &governance, &trajectory)?;
            if validation_epoch::active_validation_epoch_bounded(root)?
                != self.record.validation_epoch
            {
                return Err(invalid("diagnostic observed a changed validation epoch"));
            }
            self.record
                .proof
                .current_inputs
                .as_ref()
                .ok_or_else(|| invalid("no retained current input witness"))?
                .verify_current(root, source)?;
            let task = todo::capture_execution_custody(
                &self.store_root,
                &self.record.task_binding.task_id,
                &crate::current_agent_id(),
            )?;
            if task["revision"].as_i64() != Some(self.record.task_binding.revision)
                || task["lease_generation"].as_i64()
                    != Some(self.record.task_binding.lease_generation)
                || task["assignee"].as_str() != Some(self.record.task_binding.assignee.as_str())
            {
                return Err(invalid("diagnostic observed changed task custody"));
            }
            Ok(())
        };
        let status = if current().is_err() {
            "stale_or_unproven"
        } else if offline && !source.repository.starts_with("local-git:") {
            "historical_target_unverified"
        } else {
            "current_historical_inputs"
        };
        serde_json::json!({"status":status,"source":"canonical_full_validation_history",
            "record_id":self.record_id(),"record_digest":self.record_digest(),
            "candidate":source.head_revision,"accepted_base":source.accepted_base_revision,
            "configuration_digest":source.configuration_digest,"scope":self.record.proof.scope,
            "required_plan_digest":self.record.proof.expected_plan.digest().ok(),
            "publication_ready":false,"latest_provider_attempt":"requires_live_verification",
            "next_action":"Use workspace publish for live current-attempt validation and final readback; this diagnostic does not recreate admission"})
    }

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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn target_scope(&self) -> Option<&TargetProofScope> {
        self.record.proof.target_scope()
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

fn is_target_scoped_record(record: &CompletionRecord) -> bool {
    record.proof.target_scope.is_some()
        || matches!(
            record.proof.scope.as_str(),
            super::repository_profile::TARGET_READINESS_SCOPE
                | super::repository_profile::LOCAL_GIT_TARGET_READINESS_SCOPE
        )
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
    let record = if let Some(id) = id {
        let event =
            events::query_subject_event_on_conn(&conn, events::VERIFICATION, "commit", sha, id)?
                .ok_or_else(|| {
                    invalid("no canonical full validation proof was recorded for this commit")
                })?;
        let record = decode(&event)?;
        if is_target_scoped_record(&record) {
            return Err(invalid(
                "TARGET_SCOPED_PROOF_REJECTED: target-scoped validation record cannot satisfy full-repository readiness",
            ));
        }
        record
    } else {
        let history =
            events::query_subject_on_conn(&conn, events::VERIFICATION, "commit", sha, MAX_RECORDS)?;
        let mut binding: Option<HistoricalTaskBinding> = None;
        let mut selected = None;
        for event in &history {
            let record = decode(event)?;
            if record.candidate_sha != sha || record.proof.material.repository != repository {
                return Err(invalid(
                    "subject journal contains another commit or repository",
                ));
            }
            if is_target_scoped_record(&record) {
                continue;
            }
            if let Some(previous) = &binding {
                let next = &record.task_binding;
                if previous.task_id != next.task_id
                    || previous.contract_digest != next.contract_digest
                    || previous.assignee != next.assignee
                    || previous.lease_generation != next.lease_generation
                {
                    return Err(invalid(
                        "commit has successful records for different task contracts or custody; select the original record explicitly",
                    ));
                }
            }
            binding = Some(record.task_binding.clone());
            selected = Some(record);
        }
        selected.ok_or_else(|| {
            invalid("no canonical full validation proof was recorded for this commit")
        })?
    };
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

/// Look up historical proof for an exact candidate SHA and deployment target.
/// Full-repository records or records for other deployment targets are rejected.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn lookup_for_target(
    root: &Path,
    store: &Store,
    sha: &str,
    target: &str,
) -> Result<HistoricalValidationProof, DecapodError> {
    require_sha(sha)?;
    require_local(store, root)?;
    let path = events::canonical_db_path(&store.root);
    if !path.is_file() {
        return Err(invalid(
            "TARGET_SCOPED_PROOF_MISSING: no canonical validation proof was recorded for this commit",
        ));
    }
    let repository = repository_identity(root)?.canonical_name;
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    let history =
        events::query_subject_on_conn(&conn, events::VERIFICATION, "commit", sha, MAX_RECORDS)?;
    let mut selected = None;
    for event in &history {
        let record = decode(event)?;
        if record.candidate_sha != sha || record.proof.material.repository != repository {
            return Err(invalid(
                "subject journal contains another commit or repository",
            ));
        }
        if record
            .proof
            .target_scope
            .as_ref()
            .is_some_and(|scope| scope.target == target)
        {
            selected = Some(record);
        }
    }
    Ok(HistoricalValidationProof {
        record: selected.ok_or_else(|| {
            invalid(format!(
                "TARGET_SCOPED_PROOF_MISSING: no validation record for candidate {sha} and deployment target {target:?}"
            ))
        })?,
        store_root: store.root.clone(),
    })
}

/// Advisory read-only lookup of the latest recorded candidate SHA for `task_id`
/// in the verification event stream, used solely to enrich recovery guidance.
pub(crate) fn latest_candidate_for_task(store: &Store, task_id: &str) -> Option<String> {
    let path = events::canonical_db_path(&store.root);
    if !path.is_file() {
        return None;
    }
    let history = DbBroker::new(&store.root)
        .with_conn(
            &path,
            "decapod",
            None,
            "verify.history.candidate_hint",
            |conn| events::query_all_on_conn(conn, events::VERIFICATION),
        )
        .ok()?;
    for event in history.iter().rev().take(256) {
        if event
            .payload
            .pointer("/task_binding/task_id")
            .and_then(Value::as_str)
            == Some(task_id)
            && let Some(sha) = event.payload.get("candidate_sha").and_then(Value::as_str)
            && full_sha(sha)
        {
            return Some(sha.to_owned());
        }
    }
    None
}

/// Current custody is an exact selector, never permission to borrow a prior
/// owner's result. Generic historical lookup deliberately remains ambiguous.
fn lookup_current_workunit(
    root: &Path,
    store: &Store,
    binding: &ValidationBinding,
) -> Result<HistoricalValidationProof, DecapodError> {
    require_sha(&binding.candidate_sha)?;
    require_local(store, root)?;
    let path = events::canonical_db_path(&store.root);
    if !path.is_file() {
        return Err(invalid(
            "no canonical full validation proof was recorded for this commit",
        ));
    }
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    let history = events::query_subject_on_conn(
        &conn,
        events::VERIFICATION,
        "commit",
        &binding.candidate_sha,
        MAX_RECORDS,
    )?;
    if history.is_empty() {
        return Err(invalid(
            "no canonical full validation proof was recorded for this commit",
        ));
    }
    let mut selected = None;
    for event in &history {
        let record = decode(event)?;
        if record.candidate_sha != binding.candidate_sha
            || record.proof.material.repository != binding.repository
        {
            return Err(invalid(
                "subject journal contains another commit or repository",
            ));
        }
        if is_target_scoped_record(&record) {
            continue;
        }
        if record.task_binding == binding.task {
            selected = Some(record);
        }
    }
    Ok(HistoricalValidationProof {
        record: selected.ok_or_else(|| invalid(
            "WORKUNIT_PROOF_STALE: no successful full validation for current task and custody; obtain fresh full validation"
        ))?,
        store_root: store.root.clone(),
    })
}

/// Fence the canonical task/custody and producer reference in the same
/// transaction that accepts a VERIFIED workunit. No second connection, command
/// execution or agent-supplied success is trusted at this commit boundary.
pub(crate) fn check_workunit_append_on_conn(
    conn: &Connection,
    manifest: &super::workunit::WorkUnitManifest,
) -> Result<(), DecapodError> {
    let reference = manifest
        .validation_record
        .as_ref()
        .ok_or_else(|| invalid("WORKUNIT_PROOF_UNVERIFIABLE: no canonical reference"))?;
    require_sha(&reference.candidate_sha)?;
    let event = events::query_subject_event_on_conn(
        conn,
        events::VERIFICATION,
        "commit",
        &reference.candidate_sha,
        &reference.record_id,
    )?
    .ok_or_else(|| invalid("WORKUNIT_PROOF_UNVERIFIABLE: canonical record is missing"))?;
    let record = decode(&event)?;
    if is_target_scoped_record(&record)
        || record.record_digest != reference.record_digest
        || record.candidate_sha != reference.candidate_sha
        || record.task_binding.task_id != manifest.task_id
        || record.actor != crate::current_agent_id()
        || manifest.validation_epoch.as_ref() != Some(&record.validation_epoch)
        || record.workunit_subject.as_ref().is_none_or(|subject| {
            super::workunit::validation_contract_digest(manifest)
                .ok()
                .as_ref()
                != Some(&subject.contract_digest)
        })
    {
        return Err(invalid(
            "WORKUNIT_PROOF_UNVERIFIABLE: canonical task or observation binding differs",
        ));
    }
    todo::check_validation_binding_on_conn(conn, &record.task_binding, false)?;
    todo::ensure_task_full_validation_plan_on_conn(conn, &manifest.task_id)
}

/// Current workunit applicability comes only from the canonical full producer.
/// A reference selects a record; the caller cannot supply proof bytes or success.
pub(crate) fn current_workunit_proof(
    root: &Path,
    manifest: &super::workunit::WorkUnitManifest,
    reference: Option<(&str, &str)>,
) -> Result<HistoricalValidationProof, DecapodError> {
    ensure_full_validation_plan(&manifest.proof_plan)?;
    let store = Store {
        kind: super::store::StoreKind::Repo,
        root: super::store::find_governance_root(root).join(".decapod/data"),
    };
    let binding = capture_task_binding_inner(&store, root, false)?.ok_or_else(|| {
        invalid("WORKUNIT_PROOF_UNVERIFIABLE: no supported canonical local proof authority")
    })?;
    if binding.task.task_id != manifest.task_id
        || binding.task.assignee != crate::current_agent_id()
    {
        return Err(invalid(
            "WORKUNIT_PROOF_UNVERIFIABLE: current task or caller custody differs",
        ));
    }
    todo::ensure_task_full_validation_plan(&store.root, &manifest.task_id)?;
    let proof = if let Some((candidate, record)) = reference {
        if candidate != binding.candidate_sha {
            return Err(invalid(
                "WORKUNIT_PROOF_STALE: reference belongs to another candidate",
            ));
        }
        lookup_record(root, &store, candidate, record)?
    } else {
        lookup_current_workunit(root, &store, &binding)?
    };
    let record = &proof.record;
    if record.task_binding != binding.task
        || record.validation_epoch != binding.epoch
        || record.proof.governance_digest != binding.governance_digest
        || record.proof.trajectory_digest != binding.trajectory_digest
        || record.proof.material.repository != binding.repository
    {
        return Err(invalid(
            "WORKUNIT_PROOF_STALE: task, custody, epoch, repository or accepted governance changed",
        ));
    }
    let subject = record.workunit_subject.as_ref().ok_or_else(|| {
        invalid("WORKUNIT_PROOF_UNVERIFIABLE: historical declaration or prior validation did not observe this workunit; obtain fresh full validation")
    })?;
    if subject.contract_digest != super::workunit::validation_contract_digest(manifest)? {
        return Err(invalid(
            "WORKUNIT_PROOF_STALE: workunit intent, scope, requirements or references changed",
        ));
    }
    if manifest.status != super::workunit::WorkUnitStatus::Verified
        && super::workunit::capture_validation_subject(root, &manifest.task_id)?.as_ref()
            != Some(subject)
    {
        return Err(invalid(
            "WORKUNIT_PROOF_STALE: workunit manifest or journal generation changed after observed validation; obtain fresh full validation before promotion",
        ));
    }
    let execution = super::trajectory::capture_execution_binding(root, &manifest.task_id)?;
    if execution.intent_id != manifest.intent_ref
        || execution.head_revision != record.candidate_sha
        || execution.material_digest != record.proof.material.tree_digest
    {
        return Err(invalid(
            "WORKUNIT_PROOF_STALE: current execution binding differs from observed proof",
        ));
    }
    record
        .proof
        .current_inputs
        .as_ref()
        .ok_or_else(|| {
            invalid(
                "WORKUNIT_PROOF_UNVERIFIABLE: legacy record has no current tool/input provenance",
            )
        })?
        .verify_current(root, &record.proof.material)?;
    record.proof.verify_observed_authority(root)?;
    Ok(proof)
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
        let remote = if let Some(local_root) = &repository.local_target_root {
            let oid = workspace::read_canonical_local_git(
                local_root,
                &program,
                &["rev-parse", "--verify", &format!("{branch_ref}^{{commit}}")],
            )?;
            format!("{oid}\t{branch_ref}")
        } else {
            accepted_target_for_repository(
                root,
                &self.store_root,
                &program,
                &repository,
                &branch_ref,
            )?
        };
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

#[derive(Deserialize)]
struct GithubGitRefResponse {
    #[serde(rename = "ref")]
    ref_name: String,
    object: GithubGitRefObject,
}

#[derive(Deserialize)]
struct GithubGitRefObject {
    sha: String,
    #[serde(rename = "type")]
    object_type: String,
}

fn github_ref_api_endpoint(
    owner: &str,
    repository: &str,
    branch_ref: &str,
) -> Result<String, DecapodError> {
    let repo = super::snapshot_provider::Repository::new(owner, repository).map_err(invalid)?;
    let suffix = branch_ref
        .strip_prefix("refs/")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("accepted branch must start with refs/"))?;
    let encoded_segments: Vec<String> = suffix
        .split('/')
        .map(super::snapshot_provider::encode_path)
        .collect();
    Ok(format!(
        "{}/git/ref/{}",
        repo.api(),
        encoded_segments.join("/")
    ))
}

fn accepted_target_via_gh_runner(
    runner: &mut impl super::snapshot_provider::Runner,
    root: &Path,
    owner: &str,
    repository: &str,
    branch_ref: &str,
) -> Result<String, DecapodError> {
    let endpoint = github_ref_api_endpoint(owner, repository, branch_ref)?;
    let spec = super::snapshot_provider::CommandSpec {
        program: super::snapshot_provider::Program::Github,
        args: vec![
            "api".into(),
            "--hostname".into(),
            "github.com".into(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            "Accept: application/vnd.github+json".into(),
            endpoint.clone(),
        ],
        cwd: root.to_path_buf(),
        organization: None,
        provider_request: Some(super::snapshot_provider::ProviderRequest::GithubGet {
            path: endpoint,
        }),
    };
    let cancel = Cancellation::default();
    let budget = super::snapshot_provider::Budget::new(Duration::from_secs(30), 64 * 1024, &cancel)
        .map_err(invalid)?;
    let reply = runner
        .execute(&spec, &budget)
        .map_err(|_| invalid("authenticated GitHub CLI accepted target read did not complete"))?;
    if reply.exit_code != 0 {
        return Err(invalid(
            "authenticated GitHub CLI accepted target read failed",
        ));
    }
    let parsed: GithubGitRefResponse = serde_json::from_slice(&reply.stdout)
        .map_err(|_| invalid("authenticated GitHub CLI returned invalid branch ref payload"))?;
    if parsed.ref_name != branch_ref || parsed.object.object_type != "commit" {
        return Err(invalid(
            "authenticated GitHub CLI returned an unexpected ref or non-commit object",
        ));
    }
    require_sha(&parsed.object.sha)?;
    Ok(format!("{}\t{branch_ref}", parsed.object.sha))
}

fn accepted_target_authenticated_git(
    program: &Path,
    remote: &str,
    branch_ref: &str,
) -> Result<String, DecapodError> {
    // Isolate from checkout .git/config and hooks, but retain host-user
    // credential helper / environment lookups over strict non-redirect HTTPS.
    let scratch = tempfile::tempdir()?;
    let mut command = Command::new(program);
    command
        .current_dir(scratch.path())
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", "/dev/null")
        .env("GIT_CEILING_DIRECTORIES", scratch.path())
        .args([
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
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "GIT_ASKPASS",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
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
            "isolated host-user authenticated Git accepted target read failed",
        ));
    }
    Ok(String::from_utf8(capture.output.stdout)
        .map_err(invalid)?
        .trim()
        .to_owned())
}

fn accepted_target_for_repository(
    root: &Path,
    store_root: &Path,
    program: &Path,
    repository: &workspace::ReadinessRepository,
    branch_ref: &str,
) -> Result<String, DecapodError> {
    if let Ok(public) = accepted_target(program, &repository.remote_url, branch_ref) {
        return Ok(public);
    }
    if !repository.owner.is_empty()
        && !repository.repository.is_empty()
        && let Ok(gh_path) = crate::readiness_installed_tool(root, "gh")
    {
        let auth_store = store_root.to_path_buf();
        if let Ok(mut runner) = super::snapshot_provider::OfficialCli::new(
            program.to_path_buf(),
            gh_path,
            None,
            move |spec, executable| {
                let request = spec.authorize_provider_read()?;
                external_action::authorize_official_github_read(&auth_store, executable, &request)
                    .map_err(std::io::Error::other)
            },
        ) && let Ok(authenticated) = accepted_target_via_gh_runner(
            &mut runner,
            root,
            &repository.owner,
            &repository.repository,
            branch_ref,
        ) {
            return Ok(authenticated);
        }
    }
    if let Ok(authenticated) =
        accepted_target_authenticated_git(program, &repository.remote_url, branch_ref)
    {
        return Ok(authenticated);
    }
    Err(invalid(
        "accepted target is unavailable through reviewed public HTTPS and isolated host-user authenticated transport; check host GitHub CLI (`gh auth status`) or Git credentials and repository access (raw transport diagnostics are withheld to protect credentials)",
    ))
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
            None,
            &material,
            &hash("governance"),
            &hash("trajectory"),
            &plan,
            &suite,
            &laya,
            None,
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
            workunit_subject: None,
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
            workunit_subject: None,
        };
        let mut report = ValidationReport {
            observed_path: crate::core::trajectory::path::Judgment::unavailable(
                false,
                "fixture has no path witness",
            ),
            ready: true,
            proof_ready: true,
            laya_readiness: Some(record.proof.laya.clone()),
            scope: validation_runtime::READINESS_SCOPE,
            target: None,
            resolved_targets: vec![],
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

    #[test]
    fn target_scoped_completed_evidence_cannot_authorize_full_repo_or_other_target() {
        let mut base = record();
        let mut plan = base.proof.expected_plan.clone();
        plan.checks.truncate(1);
        plan.checks[0].kind = evidence::CheckKind::RepositoryLocal;
        plan.checks[0].executor = evidence::Provider::Native;
        plan.checks[0].key = evidence::CheckKey {
            provider: evidence::Provider::Native,
            workflow: ".decapod/config.toml".into(),
            job: "check-linux".into(),
            matrix: BTreeMap::new(),
            profile: super::super::repository_profile::REQUIRED_PROFILE.into(),
        };
        let mut result = base.proof.suite.results[0].clone();
        result.key = plan.checks[0].key.clone();
        if let evidence::Observation::Executed {
            artifact_digest, ..
        } = &mut result.observation
        {
            *artifact_digest = None;
        }
        let suite = evidence::SuiteEvidence {
            expected_plan_digest: plan.digest().unwrap(),
            results: vec![result],
            ..base.proof.suite.clone()
        };
        let mut laya = base.proof.laya.clone();
        laya.binding.proof_plan_digest = plan.digest().unwrap();
        laya.binding.evidence_digest = evidence::digest(&suite).unwrap();
        laya.request_id = evidence::digest(&laya.binding).unwrap();

        let target_scope = Some(TargetProofScope {
            target: "linux".into(),
            resolved_targets: vec!["linux".into(), "shared".into()],
        });

        // Claiming full-repo scope with target_scope present fails verification.
        assert!(
            CompletedReadinessEvidence::from_runtime(
                "target-inv",
                super::super::repository_profile::READINESS_SCOPE,
                target_scope.clone(),
                &base.proof.material,
                &base.proof.governance_digest,
                &base.proof.trajectory_digest,
                &plan,
                &suite,
                &laya,
                None,
            )
            .is_err()
        );

        let completed = CompletedReadinessEvidence::from_runtime(
            "target-inv",
            super::super::repository_profile::TARGET_READINESS_SCOPE,
            target_scope.clone(),
            &base.proof.material,
            &base.proof.governance_digest,
            &base.proof.trajectory_digest,
            &plan,
            &suite,
            &laya,
            None,
        )
        .unwrap();
        assert_eq!(completed.target_scope(), target_scope.as_ref());

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".decapod/data")).unwrap();
        std::fs::write(
            repo.join(".decapod/config.toml"),
            toml::to_string(&crate::cli::DecapodProjectConfig::default()).unwrap(),
        )
        .unwrap();
        let git_bin = crate::readiness_installed_tool(&repo, "git").unwrap();
        let run_git = |args: &[&str]| {
            let out = Command::new(&git_bin)
                .current_dir(&repo)
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "History Test")
                .env("GIT_COMMITTER_NAME", "History Test")
                .env("GIT_AUTHOR_EMAIL", "history@example.invalid")
                .env("GIT_COMMITTER_EMAIL", "history@example.invalid")
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}: {out:?}");
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        run_git(&["init", "-b", "master"]);
        run_git(&[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ]);
        run_git(&["add", "."]);
        run_git(&["commit", "-m", "init"]);
        let head = run_git(&["rev-parse", "HEAD"]);

        let mut material = base.proof.material.clone();
        material.head_revision = head.clone();
        let mut suite = suite;
        suite.before = material.clone();
        suite.after = material.clone();
        suite.results[0].identity = material.clone();
        laya.binding.material = material.clone();
        laya.binding.evidence_digest = evidence::digest(&suite).unwrap();
        laya.request_id = evidence::digest(&laya.binding).unwrap();

        let completed = CompletedReadinessEvidence::from_runtime(
            "target-inv",
            super::super::repository_profile::TARGET_READINESS_SCOPE,
            target_scope,
            &material,
            &base.proof.governance_digest,
            &base.proof.trajectory_digest,
            &plan,
            &suite,
            &laya,
            None,
        )
        .unwrap();
        base.candidate_sha = head.clone();
        base.proof = completed.clone();
        base.event_id = "target-inv".into();
        base.record_digest = base.calculated_digest().unwrap();
        base.verify().unwrap();

        let evidence_dir = tempfile::tempdir().unwrap();
        let report = ValidationReport {
            ready: true,
            proof_ready: true,
            observed_path: crate::core::trajectory::path::Judgment::unavailable(
                false,
                "fixture has no path witness",
            ),
            laya_readiness: Some(completed.laya.clone()),
            scope: super::super::repository_profile::TARGET_READINESS_SCOPE,
            target: Some("linux".into()),
            resolved_targets: vec!["linux".into(), "shared".into()],
            agent_guidance: validation_runtime::ValidationGuidance::for_outcome(true),
            stage_dispositions: vec![],
            blockers: vec![],
            evidence_path: Some(evidence_dir.path().to_path_buf()),
            completed_evidence: Some(completed),
        };
        assert!(!report.publication_ready());
        assert!(PublicationReadiness::from_report(report, None).is_err());

        // Write target-scoped record into canonical events DB and verify lookup isolation.
        let store = Store {
            kind: super::super::store::StoreKind::Repo,
            root: repo.join(".decapod/data"),
        };
        crate::subsystems::initialize_all_dbs(&store.root).unwrap();
        let event = serde_json::to_value(&base).unwrap();
        let conn =
            db::db_connect(&events::canonical_db_path(&store.root).to_string_lossy()).unwrap();
        events::ensure_tables(&conn).unwrap();
        events::append_on_conn(&conn, events::VERIFICATION, &event).unwrap();

        assert!(is_target_scoped_record(&base));
        // Full-repo lookup and explicit record lookup must reject target-scoped records.
        assert!(lookup(&repo, &store, &head).is_err());
        assert!(lookup_record(&repo, &store, &head, "target-inv").is_err());
        // Target-scoped lookup succeeds only for the exact target and SHA.
        let found = lookup_for_target(&repo, &store, &head, "linux").unwrap();
        assert_eq!(found.target_scope().unwrap().target, "linux");
        assert!(lookup_for_target(&repo, &store, &head, "macos").is_err());
        assert!(lookup_for_target(&repo, &store, &"f".repeat(40), "linux").is_err());
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/validation_history_integration_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "../../../tests/unit/core/validation_history_archive_tests.rs"]
mod archive_tests;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "../../../tests/unit/core/validation_history_workunit_tests.rs"]
pub(crate) mod workunit_observation_tests;
