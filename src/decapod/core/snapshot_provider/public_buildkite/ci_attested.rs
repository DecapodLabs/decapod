//! Successful complete provider metadata for an independently chosen CI trust
//! contract. This is not source-execution custody, and cannot select a required
//! job set, activate a policy, or create a readiness observation by itself.
use super::*;

/// A finite, independently accepted trust declaration, not a version or trigger
/// mode inferred from job names, commands, or successful provider rows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PipelineTriggerContract {
    #[serde(rename = "buildkite-gha-0.102.7")]
    BuildkiteGha01027,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PipelineTriggerPolicy {
    pub contract: PipelineTriggerContract,
    pub workflow_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency_group: Option<String>,
}

pub const PIPELINE_TRIGGER_IDENTITY: &str = "buildkite-pipeline-trigger";
pub const WORKFLOW_REF_CONCURRENCY: &str =
    "${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}";

impl PipelineTriggerPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure(
            !self.workflow_name.is_empty()
                && self.workflow_name.len() <= 256
                && self
                    .workflow_name
                    .bytes()
                    .all(|b| b.is_ascii_graphic() || b == b' ')
                && !self.workflow_name.contains("${{")
                && self.workflow_name.trim() == self.workflow_name
                && self
                    .concurrency_group
                    .as_deref()
                    .is_none_or(|v| v == WORKFLOW_REF_CONCURRENCY),
            "unsupported CI-attested Pipeline Trigger workflow/concurrency contract",
        )
    }

    fn concurrency_identity(
        &self,
        expected: &InventoryExpectation,
    ) -> Result<Option<(String, String)>> {
        self.validate()?;
        ensure(
            expected.event == "push" && branch(&format!("refs/heads/{}", expected.branch)),
            "Pipeline Trigger contract supports only branch push events",
        )?;
        super::super::github_artifacts::workflow_name(&expected.workflow_path)?;
        if self.concurrency_group.is_none() {
            return Ok(None);
        }
        // Official buildkite-gha v0.102.7, commit
        // c4227bfb48202fc4d4ba8df9fffd17f1c47c4376: workflow key is the first
        // 16 SHA-256 hex digits of the path; concurrency group is lower-cased
        // repository NUL evaluated group; job suffix binds NUL workflow key
        // NUL concurrency group. No observed step key enters these hashes.
        let workflow_hash = format!("{:x}", Sha256::digest(expected.workflow_path.as_bytes()));
        let workflow_key = format!("gha-workflow-{}", &workflow_hash[..16]);
        let group_input = format!(
            "{}\0{}",
            expected.repository.full_name().to_ascii_lowercase(),
            format!("{}-refs/heads/{}", self.workflow_name, expected.branch).to_ascii_lowercase()
        );
        let group = format!(
            "buildkite-gha/concurrency/{:x}",
            Sha256::digest(group_input.as_bytes())
        );
        let suffix = format!(
            "{:x}",
            Sha256::digest(format!("\0{workflow_key}\0{group}").as_bytes())
        );
        Ok(Some((group, suffix[..12].to_owned())))
    }

    /// Accepted auxiliary identities, derived only from policy and event inputs.
    /// The first is a bookkeeping slot for a keyless bootstrap, not a claim
    /// that the provider supplied a native key. Evidence retains its real UUID.
    pub fn auxiliary_step_keys(&self, expected: &InventoryExpectation) -> Result<Vec<String>> {
        let mut keys = vec![PIPELINE_TRIGGER_IDENTITY.to_owned()];
        if let Some((_, suffix)) = self.concurrency_identity(expected)? {
            keys.push(format!("gha-concurrency-start-{suffix}"));
            keys.push(format!("gha-concurrency-finish-{suffix}"));
        }
        Ok(keys)
    }
}

/// Sealed, in-process result of checking the exact controller-supplied run and
/// native job set. There is deliberately no deserializer or caller constructor.
/// No log, output, archive, or physical execution digest is represented here.
#[derive(Debug)]
pub struct CiAttestedValidation {
    repository: Repository,
    pipeline_id: String,
    run_id: String,
    commit: String,
    branch: String,
    workflow_path: String,
    event: String,
    jobs: BTreeMap<String, String>,
    required_jobs: BTreeMap<String, String>,
    auxiliary_jobs: BTreeMap<String, String>,
    metadata_digest: String,
    response_digests: [String; 3],
}
impl CiAttestedValidation {
    pub fn provider(&self) -> &'static str {
        "buildkite"
    }
    pub fn repository(&self) -> &Repository {
        &self.repository
    }
    /// Canonical repository identity used by material-bound readiness evidence.
    pub fn repository_identity(&self) -> String {
        format!("github.com/{}", self.repository.full_name())
    }
    pub fn pipeline_id(&self) -> &str {
        &self.pipeline_id
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn branch(&self) -> &str {
        &self.branch
    }
    pub fn workflow_path(&self) -> &str {
        &self.workflow_path
    }
    pub fn event(&self) -> &str {
        &self.event
    }
    /// Exact native step keys, plus the explicit Pipeline Trigger bookkeeping
    /// slot when selected, to actual provider job UUIDs. Every entry succeeded
    /// without retries or soft failure; the bootstrap slot is not a native key.
    pub fn jobs(&self) -> &BTreeMap<String, String> {
        &self.jobs
    }
    pub fn required_jobs(&self) -> &BTreeMap<String, String> {
        &self.required_jobs
    }
    pub fn auxiliary_jobs(&self) -> &BTreeMap<String, String> {
        &self.auxiliary_jobs
    }
    /// Domain-separated SHA-256 of the three length-prefixed original response
    /// bodies, in envelope/steps/jobs order. It is metadata evidence only.
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }
    pub fn response_digest(&self) -> &str {
        self.metadata_digest()
    }
    pub fn response_digests(&self) -> &[String; 3] {
        &self.response_digests
    }
}

pub(super) fn metadata_digest(bodies: [&[u8]; 3]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"decapod.public-buildkite.ci-metadata.v1\0");
    for body in bodies {
        digest.update((body.len() as u64).to_be_bytes());
        digest.update(body);
    }
    format!("sha256:{:x}", digest.finalize())
}

fn expected_keys<'a>(required: &'a [String], auxiliary: &'a [String]) -> Result<BTreeSet<&'a str>> {
    ensure(
        !required.is_empty() && required.len().saturating_add(auxiliary.len()) <= 2000,
        "CI-attested required native job set empty or exceeds bound",
    )?;
    let mut keys = BTreeSet::new();
    for key in required.iter().chain(auxiliary) {
        ensure(
            slug(key) && keys.insert(key.as_str()),
            "CI-attested required/auxiliary step keys invalid, repeated or overlapping",
        )?;
    }
    Ok(keys)
}

/// Check a complete inventory against independently provided controller inputs.
/// Required and auxiliary native keys must be explicit and disjoint; their
/// union must cover every native job exactly once. Missing keys, unkeyed
/// importer jobs, unexpected extras, retries, and partial success are errors.
///
/// The caller must obtain these inputs from an accepted CI-provider contract,
/// never infer required keys or the expected source/run from successful rows.
/// The contract separately binds source definitions and material identity; this
/// function does not claim to have inspected executed source or physical output.
pub fn verify_ci_attested_validation(
    inventory: &PublicInventory,
    expected: &InventoryExpectation,
    expected_run_id: &str,
    required_step_keys: &[String],
    auxiliary_step_keys: &[String],
) -> Result<CiAttestedValidation> {
    verify_ci_attested_validation_with_pipeline_trigger(
        inventory,
        expected,
        expected_run_id,
        required_step_keys,
        auxiliary_step_keys,
        None,
    )
}

/// Opt-in finite Pipeline Trigger contract. The accepted declaration authorizes
/// one keyless root bootstrap on the already authenticated configured pipeline;
/// its label is only a consistency check, never the source of that authority.
pub fn verify_ci_attested_validation_with_pipeline_trigger(
    inventory: &PublicInventory,
    expected: &InventoryExpectation,
    expected_run_id: &str,
    required_step_keys: &[String],
    auxiliary_step_keys: &[String],
    pipeline_trigger: Option<&PipelineTriggerPolicy>,
) -> Result<CiAttestedValidation> {
    let mut auxiliary_step_keys = auxiliary_step_keys.to_vec();
    if let Some(policy) = pipeline_trigger {
        auxiliary_step_keys.extend(policy.auxiliary_step_keys(expected)?);
    }
    let expected_keys = expected_keys(required_step_keys, &auxiliary_step_keys)?;
    ensure(
        uuid(expected_run_id)
            && uuid(&expected.pipeline_id)
            && oid(&expected.commit)
            && branch(&format!("refs/heads/{}", expected.branch))
            && matches!(
                expected.event.as_str(),
                "push" | "pull_request" | "workflow_dispatch"
            ),
        "invalid CI-attested expected run/source identity",
    )?;
    super::super::github_artifacts::workflow_name(&expected.workflow_path)?;
    // Rebind even an already collected inventory to this independently chosen
    // contract. An inventory for another successful run cannot be repurposed.
    verify_envelope(&inventory.envelope, &inventory.build, expected)?;
    ensure(
        inventory.build_id() == expected_run_id,
        "CI-attested run identity mismatch",
    )?;
    ensure(
        inventory.all_native_scripts_passed(),
        "CI-attested run or native job was not a complete success",
    )?;
    ensure(
        inventory.jobs.len() == expected_keys.len(),
        "CI-attested native job set incomplete or unexpected",
    )?;
    let bootstrap = if pipeline_trigger.is_some() {
        let unkeyed: Vec<_> = inventory
            .jobs
            .values()
            .filter(|job| job.step_key().is_none())
            .collect();
        ensure(
            unkeyed.len() == 1,
            "Pipeline Trigger requires exactly one keyless bootstrap",
        )?;
        let job = unkeyed[0];
        let step_id = string(job.raw(), "step_uuid")?;
        let step = &inventory.steps[step_id];
        ensure(
            field(job.raw(), "step_key")?.is_null()
                && field(step, "key")?.is_null()
                && field(job.raw(), "command")?.is_null()
                && field(step, "command_script")?.is_null()
                && field(step, "uploaded_by_step_uuid")?.is_null()
                && string(step, "label")? == ":github:"
                && string(job.raw(), "name")? == string(step, "label")?
                && field(job.raw(), "matrix")?.is_null()
                && field(job.raw(), "parallel_group_index")?.is_null()
                && field(job.raw(), "parallel_group_total")?.is_null(),
            "Pipeline Trigger bootstrap shape or label consistency mismatch",
        )?;
        for (id, child) in &inventory.steps {
            ensure(
                string(child, "state")? == "finished"
                    && string(child, "outcome")? == "passed"
                    && count(field(child, "statistics")?, "jobs_finished_count")? == 1
                    && (id == step_id || string(child, "uploaded_by_step_uuid")? == step_id),
                "Pipeline Trigger step completion or bootstrap linkage mismatch",
            )?;
        }
        Some(job.id())
    } else {
        None
    };
    let mut jobs = BTreeMap::new();
    for job in inventory.jobs.values() {
        let key = match job.step_key() {
            Some(key) => {
                ensure(
                    pipeline_trigger.is_none() || key != PIPELINE_TRIGGER_IDENTITY,
                    "native key collides with Pipeline Trigger bookkeeping identity",
                )?;
                key
            }
            None if bootstrap == Some(job.id()) => PIPELINE_TRIGGER_IDENTITY,
            None => return Err(invalid("CI-attested unkeyed native job unsupported")),
        };
        ensure(
            expected_keys.contains(key)
                && jobs.insert(key.to_owned(), job.id().to_owned()).is_none(),
            "CI-attested unexpected or duplicate native step key",
        )?;
    }
    ensure(
        jobs.keys().map(String::as_str).collect::<BTreeSet<_>>() == expected_keys,
        "CI-attested required native job missing",
    )?;
    if let Some(policy) = pipeline_trigger
        && let Some((group, suffix)) = policy.concurrency_identity(expected)?
    {
        for phase in ["start", "finish"] {
            let key = format!("gha-concurrency-{phase}-{suffix}");
            let job = inventory
                .jobs
                .get(&jobs[&key])
                .expect("verified job identity");
            ensure(
                string(job.raw(), "concurrency_group")? == group
                    && count(job.raw(), "concurrency")? == 1
                    && string(job.raw(), "command")? == "true",
                "Pipeline Trigger concurrency gate shape mismatch",
            )?;
        }
    }
    let select = |keys: &[String]| {
        keys.iter()
            .map(|key| (key.clone(), jobs[key].clone()))
            .collect()
    };
    Ok(CiAttestedValidation {
        repository: expected.repository.clone(),
        pipeline_id: string(field(&inventory.envelope, "project")?, "id")?.into(),
        run_id: inventory.build_id().into(),
        commit: string(&inventory.envelope, "commit_id")?.into(),
        branch: string(&inventory.envelope, "branch_name")?.into(),
        workflow_path: string(&inventory.envelope, "github_workflow_path")?.into(),
        event: string(&inventory.envelope, "github_webhook_event")?.into(),
        required_jobs: select(required_step_keys),
        auxiliary_jobs: select(&auxiliary_step_keys),
        jobs,
        metadata_digest: inventory.metadata_digest.clone(),
        response_digests: inventory.response_digests.clone(),
    })
}

/// Fresh metadata only: exactly the existing envelope, full steps, and full jobs
/// routes. This does not download logs/artifacts, change authentication, choose
/// winning runs, or replace the independently configured validation contract.
pub fn collect_ci_attested_validation(
    reader: &mut impl PublicReader,
    build: PublicBuild,
    expected: &InventoryExpectation,
    expected_run_id: &str,
    required_step_keys: &[String],
    auxiliary_step_keys: &[String],
    budget: &Budget<'_>,
) -> Result<CiAttestedValidation> {
    collect_ci_attested_validation_with_pipeline_trigger(
        reader,
        build,
        expected,
        expected_run_id,
        required_step_keys,
        auxiliary_step_keys,
        None,
        budget,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn collect_ci_attested_validation_with_pipeline_trigger(
    reader: &mut impl PublicReader,
    build: PublicBuild,
    expected: &InventoryExpectation,
    expected_run_id: &str,
    required_step_keys: &[String],
    auxiliary_step_keys: &[String],
    pipeline_trigger: Option<&PipelineTriggerPolicy>,
    budget: &Budget<'_>,
) -> Result<CiAttestedValidation> {
    // Reject an invalid obligation set before performing any provider reads.
    let mut auxiliary = auxiliary_step_keys.to_vec();
    if let Some(policy) = pipeline_trigger {
        auxiliary.extend(policy.auxiliary_step_keys(expected)?);
    }
    expected_keys(required_step_keys, &auxiliary)?;
    ensure(
        uuid(expected_run_id),
        "invalid CI-attested expected run UUID",
    )?;
    let inventory = collect_inventory(reader, build, expected, budget)?;
    let result = verify_ci_attested_validation_with_pipeline_trigger(
        &inventory,
        expected,
        expected_run_id,
        required_step_keys,
        auxiliary_step_keys,
        pipeline_trigger,
    )?;
    budget.remaining()?;
    Ok(result)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod real_metadata_tests;
