//! Complete provider execution under an independently accepted CI policy. These
//! records describe trusted CI status, not locally measured process output,
//! downloaded artifacts, or measured remote toolchain bytes. Deserializing one
//! is not authentication: the adapter must verify the provider's complete native
//! job inventory, exact source/configuration and absence of retried or masked
//! failures before constructing it. Candidate-controlled policy cannot grant
//! this authority; the trusted ExpectedCheck pins the whole contract digest.

use super::*;

pub const BUILDKITE_CI_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildkiteRequiredJob {
    /// Exact native step key, independently mapped from the required source graph.
    pub step_key: String,
    /// Accepted job/command/configuration definition, never a response/output hash.
    pub definition_digest: String,
}

/// Frozen by the trusted controller before accepting results. Required jobs
/// include explicitly accepted auxiliary jobs; the provider cannot select or
/// shrink this set. An accepted complete CI contract may prove a candidate build
/// occurred without claiming to possess or hash its produced executable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildkiteCiContract {
    pub schema_version: String,
    /// Every material identity field except the separately bound aggregate toolchain.
    pub material_digest: String,
    /// Independently captured current intent/governance, not provider assertions.
    pub intent_digest: String,
    pub key: CheckKey,
    pub event: String,
    pub source_definition_digest: String,
    /// Declared accepted executor/configuration policy, NOT measured toolchain bytes.
    pub executor_policy_digest: String,
    pub pipeline_id: String,
    /// Exact immutable provider build UUID pinned by the independent run selection.
    pub run_id: String,
    pub required_jobs: Vec<BuildkiteRequiredJob>,
}

impl BuildkiteCiContract {
    /// Required jobs form a set, but duplicate declarations remain invalid.
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        let mut contract = self.clone();
        contract
            .required_jobs
            .sort_by(|left, right| left.step_key.cmp(&right.step_key));
        digest(&contract)
    }
}

/// Excludes only the aggregate remote toolchain field to avoid a binding-order
/// dependency: the contract's executor policy is separately pinned to the
/// ExpectedCheck, and CheckResult/SuiteEvidence still bind the final full identity.
/// All source, history, target, configuration and candidate material stays exact.
pub fn buildkite_ci_material_digest(
    identity: &MaterialIdentity,
) -> Result<String, serde_json::Error> {
    digest(&(
        "buildkite-ci-material-v1",
        &identity.repository,
        &identity.work_branch,
        &identity.head_revision,
        &identity.base_branch,
        &identity.accepted_base_revision,
        &identity.history_digest,
        &identity.tree_digest,
        &identity.configuration_digest,
        &identity.workflow_digest,
        &identity.candidate_digest,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildkiteJobObservation {
    pub step_key: String,
    pub job_id: String,
    /// The adapter must establish terminal success without soft/masked failure.
    pub conclusion: Conclusion,
}

/// The adapter authenticates these observations against the exact immutable run.
/// Buildkite's supported public inventory has no run-attempt identity; instead
/// it must reject any retry count, retried/replacement job, or pending retry.
/// We intentionally do not invent an attempt number, exit code, output digest,
/// log digest or artifact digest from a successful provider status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildkiteCiAttestation {
    pub contract: BuildkiteCiContract,
    pub observed_repository: String,
    pub observed_commit: String,
    pub observed_branch: String,
    pub observed_pipeline_id: String,
    pub observed_workflow: String,
    pub observed_event: String,
    pub observed_run_id: String,
    pub run_conclusion: Conclusion,
    /// Complete exact contract job set, including accepted auxiliary jobs.
    pub jobs: Vec<BuildkiteJobObservation>,
    /// Hash over actual provider response bytes; custody only, never test output.
    pub response_digest: String,
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
        && value.bytes().any(|byte| byte != b'0' && byte != b'-')
}

pub(super) fn valid_attestation(
    check: &ExpectedCheck,
    current: &MaterialIdentity,
    attestation: &BuildkiteCiAttestation,
) -> bool {
    let contract = &attestation.contract;
    if check.executor != Provider::Buildkite
        || check.key.provider != Provider::Buildkite
        || !matches!(check.kind, CheckKind::SourceCi | CheckKind::CandidateBuild)
        || contract.schema_version != BUILDKITE_CI_SCHEMA_VERSION
        || contract.digest().ok().as_ref() != Some(&check.invocation_digest)
        || buildkite_ci_material_digest(current).ok().as_ref() != Some(&contract.material_digest)
        || !is_digest(&contract.intent_digest)
        || contract.key != check.key
        || !nonempty(&contract.event)
        || contract.source_definition_digest != check.definition_digest
        || contract.executor_policy_digest != check.toolchain_digest
        || !uuid(&contract.pipeline_id)
        || !uuid(&contract.run_id)
        || attestation.observed_repository != current.repository
        || attestation.observed_commit != current.head_revision
        || attestation.observed_branch != current.work_branch
        || attestation.observed_pipeline_id != contract.pipeline_id
        || attestation.observed_workflow != contract.key.workflow
        || attestation.observed_event != contract.event
        || attestation.observed_run_id != contract.run_id
        || attestation.run_conclusion != Conclusion::Success
        || !is_digest(&attestation.response_digest)
        || contract.required_jobs.is_empty()
        || contract.required_jobs.len() != attestation.jobs.len()
    {
        return false;
    }
    let mut required = BTreeSet::new();
    if !contract.required_jobs.iter().all(|job| {
        nonempty(&job.step_key)
            && is_digest(&job.definition_digest)
            && required.insert(job.step_key.as_str())
    }) {
        return false;
    }
    let mut observed = BTreeSet::new();
    let mut job_ids = BTreeSet::new();
    attestation.jobs.iter().all(|job| {
        uuid(&job.job_id)
            && job_ids.insert(job.job_id.as_str())
            && observed.insert(job.step_key.as_str())
            && required.contains(job.step_key.as_str())
            && job.conclusion == Conclusion::Success
    }) && observed == required
}

#[cfg(test)]
mod tests;
