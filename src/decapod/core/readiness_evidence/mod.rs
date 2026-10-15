//! Code-bound complete-suite evidence. This is a verification primitive, not a
//! workflow parser, proof runner, signature verifier, or publication gate.
//!
//! The caller MUST independently derive the expected plan from all applicable
//! source CI definitions and project policy, capture current identity/facts, and
//! authenticate observations. Never construct the expected plan from receipts.
//! All applicable check/matrix rows and the real candidate build
//! belong in that plan. Unknown workflow semantics must fail during planning.
//! A successful receipt alone establishes neither execution nor trusted custody.
//!
//! This first schema intentionally rejects reuse. A later reuse path must verify
//! exact accepted-base full-suite evidence under the same policy, a real immutable
//! version-only classification, and a fresh candidate build. A skipped wrapper or
//! successful validator job is never evidence that its underlying checks ran.
//! A separate CI-attested mode trusts an independently accepted complete provider
//! job contract; it makes no local output or downloaded-artifact measurement claim.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: &str = "1.1.0";
pub const GITHUB_EXECUTION_SCHEMA_VERSION: &str = "1.0.0";

mod buildkite_ci;
pub use buildkite_ci::{
    BUILDKITE_CI_SCHEMA_VERSION, BuildkiteCiAttestation, BuildkiteCiContract,
    BuildkiteJobObservation, BuildkiteRequiredJob, buildkite_ci_material_digest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MaterialIdentity {
    pub repository: String,
    pub work_branch: String,
    pub head_revision: String,
    pub base_branch: String,
    pub accepted_base_revision: String,
    /// Commit range, parent graph, and message content used by history checks.
    pub history_digest: String,
    /// Full working material, including modes, deletions and declared untracked inputs.
    pub tree_digest: String,
    pub configuration_digest: String,
    pub workflow_digest: String,
    /// Aggregate execution bindings: measured tools/images/actions for measured
    /// execution, or explicitly declared accepted executor policy for CiAttested.
    /// CI policy identities are not claims to have measured remote tool bytes.
    pub toolchain_digest: String,
    /// Candidate package/build inputs; the actual built artifact is recorded below.
    pub candidate_digest: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    GithubActions,
    Buildkite,
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CheckKey {
    /// The source provider is retained when native execution replaces Buildkite.
    pub provider: Provider,
    pub workflow: String,
    pub job: String,
    pub matrix: BTreeMap<String, String>,
    pub profile: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// Repository-authored local command; never a hosted CI or candidate-build claim.
    RepositoryLocal,
    SourceCi,
    DerivedRelease,
    CandidateBuild,
    /// Explicitly rejected: validate cannot depend on its own eventual success.
    Validator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(
    tag = "kind",
    content = "check",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Dependency {
    Check(CheckKey),
    ValidationReceipt,
    Validator,
}

/// Facts come from the current checkout/event, independently of evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicabilityFacts {
    pub event: String,
    pub target_branch: String,
    /// Full base-to-candidate material change inventory, not receipt-reported paths.
    pub changed_paths: BTreeSet<String>,
}

/// Deliberately small proven non-applicability vocabulary. Unsupported expressions
/// must not be translated into an asserted boolean. No version-only shortcut.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NonApplicability {
    EventOutside {
        events: BTreeSet<String>,
    },
    TargetOutside {
        branches: BTreeSet<String>,
    },
    PathsUntouched {
        exact_paths: BTreeSet<String>,
        /// Directory names without a trailing slash, matched at component boundaries.
        directories: BTreeSet<String>,
    },
}

impl NonApplicability {
    fn proven(&self, facts: &ApplicabilityFacts) -> bool {
        match self {
            Self::EventOutside { events } => {
                !events.is_empty()
                    && events.iter().all(|v| nonempty(v))
                    && !events.contains(&facts.event)
            }
            Self::TargetOutside { branches } => {
                !branches.is_empty()
                    && branches.iter().all(|v| nonempty(v))
                    && !branches.contains(&facts.target_branch)
            }
            Self::PathsUntouched {
                exact_paths,
                directories,
            } => {
                !(exact_paths.is_empty() && directories.is_empty())
                    && exact_paths.iter().chain(directories).all(|p| valid_path(p))
                    && facts.changed_paths.iter().all(|changed| {
                        !exact_paths.contains(changed)
                            && !directories.iter().any(|dir| {
                                changed == dir || changed.starts_with(&format!("{dir}/"))
                            })
                    })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    content = "predicate",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Requirement {
    Execute,
    NotApplicable(NonApplicability),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExpectedCheck {
    pub key: CheckKey,
    pub kind: CheckKind,
    pub definition_digest: String,
    pub invocation_digest: String,
    /// Measured tool closure, or the accepted executor policy for CiAttested.
    pub toolchain_digest: String,
    pub executor: Provider,
    pub dependencies: Vec<Dependency>,
    pub requirement: Requirement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExpectedPlan {
    pub schema_version: String,
    pub policy_digest: String,
    /// Digest of the independently discovered complete source workflow inventory.
    pub source_inventory_digest: String,
    pub buildkite_enabled: bool,
    pub checks: Vec<ExpectedCheck>,
}

impl ExpectedPlan {
    /// Order-independent but never silently deduplicates obligations.
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        let mut plan = self.clone();
        plan.checks.sort_by(|a, b| a.key.cmp(&b.key));
        for check in &mut plan.checks {
            check.dependencies.sort();
        }
        digest(&plan)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Conclusion {
    Pending,
    Running,
    Success,
    Failure,
    Skipped,
    Cancelled,
    TimedOut,
}

/// Explicit provider execution semantics, separate from measured process exits.
/// Only reviewed substantive steps that propagate failures may use this mode.
/// A continue-on-error action, `|| true`, swallowed child failure, or unknown
/// reusable-action behavior is not representable as an accepted failure mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GithubFailureMode {
    RequiredTerminalSuccess,
    ContinueOnError,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubRequiredStep {
    pub name: String,
    /// Reviewed resolved command/action definition, including failure propagation.
    pub definition_digest: String,
    pub failure_mode: GithubFailureMode,
}

/// Independently planned before collecting results. Its digest is the expected
/// check's invocation_digest. Do not derive this contract from returned steps or
/// shrink it to whichever steps happened to pass. Order and matrix are material.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubExecutionContract {
    pub schema_version: String,
    pub repository: String,
    pub workflow: String,
    /// Stable source job ID; provider display name is separately bound below.
    pub job: String,
    pub provider_job_name: String,
    pub matrix: BTreeMap<String, String>,
    pub profile: String,
    pub source_definition_digest: String,
    pub source_head: String,
    pub accepted_base_revision: String,
    /// Snapshot S is transport custody only; actual source candidate is H.
    pub transport_revision: String,
    /// Immutable Git source tree object ID, not a response/log/artifact hash.
    pub source_tree: String,
    pub required_steps: Vec<GithubRequiredStep>,
}
impl GithubExecutionContract {
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        digest(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubStepObservation {
    pub name: String,
    /// Mapper may emit Success only for status=completed, conclusion=success.
    pub conclusion: Conclusion,
}

/// Accepted only after the provider adapter authenticates the exact run/attempt/
/// job, snapshot S/H/tree/base relation, reviewed workflow/step definitions and
/// non-masking failure behavior. This struct's deserialization is not custody.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubExecutionAttestation {
    pub contract: GithubExecutionContract,
    pub run_id: u64,
    pub run_attempt: u32,
    pub job_id: u64,
    pub observed_run_head: String,
    pub observed_job_name: String,
    /// Success means completed/success, never a queued/running successful attempt.
    pub run_conclusion: Conclusion,
    pub job_conclusion: Conclusion,
    /// Exactly the required substantive steps, in their observed execution order.
    pub steps: Vec<GithubStepObservation>,
    /// Hash of the real authenticated API response bundle (custody, not output).
    pub response_digest: String,
    /// Hash of actual bounded exact-job logs retrieved for this job ID.
    pub log_digest: String,
    /// Actual authenticated artifact bytes, never synthesized from API success.
    pub artifact_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    /// Accepted provider execution, without fabricated process/artifact measurements.
    CiAttested {
        attestation: Box<BuildkiteCiAttestation>,
    },
    /// Provider execution attestation; intentionally has no invented exit code.
    GithubActions {
        attestation: Box<GithubExecutionAttestation>,
    },
    Executed {
        /// A provider execution/attempt identity, or the native runner's identity.
        execution_id: String,
        conclusion: Conclusion,
        exit_code: Option<i32>,
        output_digest: String,
        /// Required for candidate builds: the actual candidate executable/package.
        artifact_digest: Option<String>,
    },
    NotApplicable {
        predicate_digest: String,
    },
    /// Kept explicit so imported reuse cannot masquerade as an executed result.
    Reused {
        evidence_digest: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub key: CheckKey,
    pub definition_digest: String,
    pub invocation_digest: String,
    pub toolchain_digest: String,
    pub executor: Provider,
    /// Each row is bound, not only the aggregate envelope.
    pub identity: MaterialIdentity,
    pub observation: Observation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SuiteEvidence {
    pub schema_version: String,
    pub expected_plan_digest: String,
    pub before: MaterialIdentity,
    pub after: MaterialIdentity,
    pub facts: ApplicabilityFacts,
    pub results: Vec<CheckResult>,
}

/// Detailed internal diagnostics; only Ready/NotReady crosses the verdict seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    UnsupportedSchema,
    InvalidIdentity,
    InvalidFacts,
    EmptyPlan,
    IncompletePlan,
    InvalidPlan,
    PlanMismatch,
    StaleIdentity,
    WorktreeChanged,
    DuplicateExpected(CheckKey),
    DuplicateResult(CheckKey),
    Missing(CheckKey),
    Extra(CheckKey),
    ForbiddenDependency(CheckKey),
    InvalidDependency(CheckKey),
    DependencyCycle,
    InvalidResult(CheckKey),
    InvalidProviderAttestation(CheckKey),
    NotExecutedSuccess(CheckKey),
    NonApplicabilityUnproven(CheckKey),
    ReuseUnsupported(CheckKey),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub verdict: Verdict,
    pub failures: Vec<Failure>,
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

fn is_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn valid_identity(identity: &MaterialIdentity) -> bool {
    [
        &identity.repository,
        &identity.work_branch,
        &identity.base_branch,
    ]
    .into_iter()
    .all(|v| nonempty(v))
        && identity.work_branch != identity.base_branch
        && is_revision(&identity.head_revision)
        && is_revision(&identity.accepted_base_revision)
        && [
            &identity.history_digest,
            &identity.tree_digest,
            &identity.configuration_digest,
            &identity.workflow_digest,
            &identity.toolchain_digest,
            &identity.candidate_digest,
        ]
        .into_iter()
        .all(|v| is_digest(v))
}

fn valid_key(key: &CheckKey) -> bool {
    [&key.workflow, &key.job, &key.profile]
        .into_iter()
        .all(|v| nonempty(v))
        && key.matrix.iter().all(|(k, v)| nonempty(k) && nonempty(v))
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && p != ".git")
}

pub fn digest<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    ))
}

fn valid_github_attestation(
    check: &ExpectedCheck,
    current: &MaterialIdentity,
    attestation: &GithubExecutionAttestation,
) -> bool {
    let contract = &attestation.contract;
    let nonzero_revision = |value: &str| is_revision(value) && value.bytes().any(|b| b != b'0');
    let mut names = BTreeSet::new();
    check.executor == Provider::GithubActions
        && contract.schema_version == GITHUB_EXECUTION_SCHEMA_VERSION
        && contract.digest().ok().as_ref() == Some(&check.invocation_digest)
        && contract.repository == current.repository
        && contract.workflow == check.key.workflow
        && contract.job == check.key.job
        && contract.matrix == check.key.matrix
        && contract.profile == check.key.profile
        && contract.source_definition_digest == check.definition_digest
        && contract.source_head == current.head_revision
        && contract.accepted_base_revision == current.accepted_base_revision
        && nonzero_revision(&contract.transport_revision)
        && nonzero_revision(&contract.source_tree)
        && nonempty(&contract.provider_job_name)
        && attestation.observed_run_head == contract.transport_revision
        && attestation.observed_job_name == contract.provider_job_name
        && attestation.run_id != 0
        && attestation.run_attempt != 0
        && attestation.job_id != 0
        && attestation.run_conclusion == Conclusion::Success
        && attestation.job_conclusion == Conclusion::Success
        && is_digest(&attestation.response_digest)
        && is_digest(&attestation.log_digest)
        && attestation
            .artifact_digest
            .as_ref()
            .is_none_or(|d| is_digest(d))
        && (check.kind != CheckKind::CandidateBuild || attestation.artifact_digest.is_some())
        && !contract.required_steps.is_empty()
        && contract.required_steps.len() == attestation.steps.len()
        && contract
            .required_steps
            .iter()
            .zip(&attestation.steps)
            .all(|(required, observed)| {
                nonempty(&required.name)
                    && names.insert(&required.name)
                    && is_digest(&required.definition_digest)
                    && required.failure_mode == GithubFailureMode::RequiredTerminalSuccess
                    && required.name == observed.name
                    && observed.conclusion == Conclusion::Success
            })
}

/// Pure, deterministic, fail-closed. `plan`, `current`, and `facts` are trusted
/// inputs independently obtained by the caller, never taken from `evidence`.
/// This function neither executes a command nor reads a receipt/worktree.
pub fn verify(
    plan: &ExpectedPlan,
    current: &MaterialIdentity,
    facts: &ApplicabilityFacts,
    evidence: &SuiteEvidence,
) -> Verification {
    let mut failures = Vec::new();
    if plan.schema_version != SCHEMA_VERSION || evidence.schema_version != SCHEMA_VERSION {
        failures.push(Failure::UnsupportedSchema);
    }
    if !valid_identity(current) {
        failures.push(Failure::InvalidIdentity);
    }
    if !nonempty(&facts.event)
        || !nonempty(&facts.target_branch)
        || facts.changed_paths.iter().any(|p| !valid_path(p))
        || &evidence.facts != facts
    {
        failures.push(Failure::InvalidFacts);
    }
    if plan.checks.is_empty() {
        failures.push(Failure::EmptyPlan);
    }
    // A distinct local-contract plan cannot masquerade as provider CI or an
    // artifact-producing candidate build. No mixed scopes, skipped local checks
    // or remote execution are admissible. Strict source-PR plans retain their
    // complete suite and mandatory real candidate build exactly as before.
    let repository_local = plan
        .checks
        .iter()
        .any(|c| c.kind == CheckKind::RepositoryLocal);
    if repository_local {
        if plan.buildkite_enabled
            || plan.checks.iter().any(|c| {
                c.kind != CheckKind::RepositoryLocal
                    || c.executor != Provider::Native
                    || c.key.provider != Provider::Native
                    || c.key.profile != crate::core::repository_profile::REQUIRED_PROFILE
                    || c.requirement != Requirement::Execute
            })
        {
            failures.push(Failure::InvalidPlan);
        }
    } else {
        for kind in [CheckKind::SourceCi, CheckKind::CandidateBuild] {
            if !plan.checks.iter().any(|c| {
                c.kind == kind
                    && (kind != CheckKind::CandidateBuild || c.requirement == Requirement::Execute)
            }) {
                failures.push(Failure::IncompletePlan);
            }
        }
    }
    if !is_digest(&plan.policy_digest) || !is_digest(&plan.source_inventory_digest) {
        failures.push(Failure::InvalidPlan);
    }
    if plan.digest().ok().as_ref() != Some(&evidence.expected_plan_digest) {
        failures.push(Failure::PlanMismatch);
    }
    if &evidence.before != current || &evidence.after != current {
        failures.push(Failure::StaleIdentity);
    }
    if evidence.before != evidence.after {
        failures.push(Failure::WorktreeChanged);
    }

    let mut expected = BTreeMap::new();
    // Sort diagnostics independently of input array order.
    let mut ordered_checks: Vec<_> = plan.checks.iter().collect();
    ordered_checks.sort_by(|a, b| a.key.cmp(&b.key));
    for check in ordered_checks {
        if expected.insert(check.key.clone(), check).is_some() {
            failures.push(Failure::DuplicateExpected(check.key.clone()));
        }
        if !valid_key(&check.key)
            || !is_digest(&check.definition_digest)
            || !is_digest(&check.invocation_digest)
            || !is_digest(&check.toolchain_digest)
            || (!plan.buildkite_enabled && check.executor == Provider::Buildkite)
        {
            failures.push(Failure::InvalidPlan);
        }
        if check.kind == CheckKind::Validator
            || check
                .dependencies
                .iter()
                .any(|d| matches!(d, Dependency::Validator | Dependency::ValidationReceipt))
        {
            failures.push(Failure::ForbiddenDependency(check.key.clone()));
        }
        if check.key.provider == Provider::Buildkite
            && !plan.buildkite_enabled
            && check.executor != Provider::Native
        {
            failures.push(Failure::InvalidPlan);
        }
    }
    // Topological elimination rejects missing, duplicate, self and cyclic edges.
    let mut remaining = BTreeMap::<CheckKey, BTreeSet<CheckKey>>::new();
    for (key, check) in &expected {
        let mut dependencies = BTreeSet::new();
        for dependency in &check.dependencies {
            if let Dependency::Check(dependency) = dependency
                && (dependency == key
                    || !expected.contains_key(dependency)
                    || !dependencies.insert(dependency.clone()))
            {
                failures.push(Failure::InvalidDependency(key.clone()));
            }
        }
        remaining.insert(key.clone(), dependencies);
    }
    loop {
        let done: BTreeSet<_> = remaining
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(key, _)| key.clone())
            .collect();
        if done.is_empty() {
            break;
        }
        remaining.retain(|key, _| !done.contains(key));
        for deps in remaining.values_mut() {
            deps.retain(|key| !done.contains(key));
        }
    }
    if !remaining.is_empty() {
        failures.push(Failure::DependencyCycle);
    }

    let mut observed = BTreeMap::new();
    let mut ordered_results: Vec<_> = evidence.results.iter().collect();
    ordered_results.sort_by(|a, b| a.key.cmp(&b.key));
    for result in ordered_results {
        if observed.insert(result.key.clone(), result).is_some() {
            failures.push(Failure::DuplicateResult(result.key.clone()));
        }
        if !expected.contains_key(&result.key) {
            failures.push(Failure::Extra(result.key.clone()));
        }
    }
    for (key, check) in expected {
        let Some(result) = observed.get(&key) else {
            failures.push(Failure::Missing(key));
            continue;
        };
        if result.identity != *current
            || result.definition_digest != check.definition_digest
            || result.invocation_digest != check.invocation_digest
            || result.toolchain_digest != check.toolchain_digest
            || result.executor != check.executor
        {
            failures.push(Failure::InvalidResult(key.clone()));
        }
        match (&check.requirement, &result.observation) {
            (_, Observation::Reused { .. }) => failures.push(Failure::ReuseUnsupported(key)),
            (Requirement::Execute, Observation::CiAttested { attestation }) => {
                if !buildkite_ci::valid_attestation(check, current, attestation) {
                    failures.push(Failure::InvalidProviderAttestation(key));
                }
            }
            (Requirement::Execute, Observation::GithubActions { attestation }) => {
                if !valid_github_attestation(check, current, attestation) {
                    failures.push(Failure::InvalidProviderAttestation(key));
                }
            }
            (
                Requirement::Execute,
                Observation::Executed {
                    execution_id,
                    conclusion,
                    exit_code,
                    output_digest,
                    artifact_digest,
                },
            ) => {
                if *conclusion != Conclusion::Success
                    || *exit_code != Some(0)
                    || !nonempty(execution_id)
                    || !is_digest(output_digest)
                    || artifact_digest.as_ref().is_some_and(|d| !is_digest(d))
                    || (check.kind == CheckKind::CandidateBuild && artifact_digest.is_none())
                {
                    failures.push(Failure::NotExecutedSuccess(key));
                }
            }
            (
                Requirement::NotApplicable(predicate),
                Observation::NotApplicable { predicate_digest },
            ) => {
                if check.kind == CheckKind::CandidateBuild
                    || !predicate.proven(facts)
                    || digest(predicate).ok().as_ref() != Some(predicate_digest)
                {
                    failures.push(Failure::NonApplicabilityUnproven(key));
                }
            }
            (Requirement::Execute, _) => failures.push(Failure::NotExecutedSuccess(key)),
            (Requirement::NotApplicable(_), _) => {
                failures.push(Failure::NonApplicabilityUnproven(key))
            }
        }
    }
    Verification {
        verdict: if failures.is_empty() {
            Verdict::Ready
        } else {
            Verdict::NotReady
        },
        failures,
    }
}

pub mod material;

#[cfg(test)]
mod tests;
