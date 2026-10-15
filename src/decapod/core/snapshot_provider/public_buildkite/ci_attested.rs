//! Successful complete provider metadata for an independently chosen CI trust
//! contract. This is not source-execution custody, and cannot select a required
//! job set, activate a policy, or create a readiness observation by itself.
use super::*;

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
    /// Exact native step key to provider job UUID, including explicitly accepted
    /// auxiliary jobs. Every entry succeeded without retries or soft failure.
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
    let expected_keys = expected_keys(required_step_keys, auxiliary_step_keys)?;
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
    let mut jobs = BTreeMap::new();
    for job in inventory.jobs.values() {
        let key = job
            .step_key()
            .ok_or_else(|| invalid("CI-attested unkeyed native job unsupported"))?;
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
        auxiliary_jobs: select(auxiliary_step_keys),
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
    // Reject an invalid obligation set before performing any provider reads.
    expected_keys(required_step_keys, auxiliary_step_keys)?;
    ensure(
        uuid(expected_run_id),
        "invalid CI-attested expected run UUID",
    )?;
    let inventory = collect_inventory(reader, build, expected, budget)?;
    let result = verify_ci_attested_validation(
        &inventory,
        expected,
        expected_run_id,
        required_step_keys,
        auxiliary_step_keys,
    )?;
    budget.remaining()?;
    Ok(result)
}

#[cfg(test)]
mod tests;
