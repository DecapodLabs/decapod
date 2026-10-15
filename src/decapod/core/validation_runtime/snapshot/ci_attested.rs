//! Explicit accepted-policy alternative to archive custody. This trusts the
//! configured CI executor's job semantics; metadata is not measured binary proof.
use super::*;
use provider::public_buildkite as bk;

pub const POLICY_SCHEMA: &str = "decapod.ci-attested-validation.v1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    pub workflows: Vec<Workflow>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub path: String,
    pub required_step_keys: Vec<String>,
    pub auxiliary_step_keys: Vec<String>,
    pub checks: Vec<CheckMapping>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckMapping {
    pub check: CheckKey,
    pub step_key: String,
}
/// A candidate cannot choose a weaker verifier for its own publication.
pub(crate) fn verify_policy_acceptance(
    policy: &Policy,
    accepted: Option<&Policy>,
    buildkite: bool,
) -> io::Result<()> {
    if !buildkite || accepted != Some(policy) {
        return Err(invalid(
            "CI_ATTESTED_POLICY_NOT_ACCEPTED: the exact policy must already be accepted on the target; candidate policy changes cannot authorize themselves",
        ));
    }
    if policy.schema != POLICY_SCHEMA || policy.workflows.is_empty() {
        return Err(invalid(
            "invalid CI-attested policy schema or empty workflow set",
        ));
    }
    let mut workflows = BTreeSet::new();
    let mut checks = BTreeSet::new();
    for workflow in &policy.workflows {
        if !workflows.insert(&workflow.path)
            || workflow.required_step_keys.is_empty()
            || workflow.checks.is_empty()
        {
            return Err(invalid("empty or duplicate CI-attested workflow"));
        }
        let mut keys = BTreeSet::new();
        for key in workflow
            .required_step_keys
            .iter()
            .chain(&workflow.auxiliary_step_keys)
        {
            if key.is_empty() || !keys.insert(key) {
                return Err(invalid("empty or duplicate CI-attested step"));
            }
        }
        for mapping in &workflow.checks {
            if mapping.check.provider != evidence::Provider::Buildkite
                || mapping.check.workflow != workflow.path
                || !workflow.required_step_keys.contains(&mapping.step_key)
                || !checks.insert(&mapping.check)
            {
                return Err(invalid("invalid or duplicate CI-attested check mapping"));
            }
        }
    }
    Ok(())
}

pub(super) fn collect(
    reader: &mut impl bk::PublicReader,
    snapshot: &transport::PreparedSnapshot,
    project: &CurrentProject,
    terminal: &automatic::TerminalSourceRuns,
    identity: &evidence::MaterialIdentity,
    policy: &Policy,
    budget: &provider::Budget<'_>,
) -> io::Result<source_import::SourceCollection> {
    let mut output = source_import::SourceCollection::default();
    let policy_digest = evidence::digest(policy).map_err(io::Error::other)?;
    let plan = crate::plan_governance::load_plan(&project.identity_inputs.root)
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing declared intent"))?;
    if plan.intent.trim().is_empty() {
        return Err(invalid("missing declared intent"));
    }
    let intent_digest = evidence::digest(&plan.intent).map_err(io::Error::other)?;
    let expected_workflows: BTreeSet<_> = project
        .catalog
        .obligations()
        .iter()
        .map(|o| o.key.workflow.as_str())
        .collect();
    let declared_workflows: BTreeSet<_> =
        policy.workflows.iter().map(|w| w.path.as_str()).collect();
    if expected_workflows != declared_workflows {
        return Err(invalid(
            "CI-attested workflow set differs from complete required catalog",
        ));
    }
    for workflow in &policy.workflows {
        let pinned = terminal
            .buildkite("decapodlabs", "decapod", &workflow.path)
            .ok_or_else(|| invalid("required validation workflow absent"))?;
        let expectation = bk::InventoryExpectation {
            repository: snapshot.request().repository.clone(),
            pipeline_id: "01a0bcf1-5e16-44ae-8457-04495fae8ff5".into(),
            commit: snapshot.transport_head().into(),
            branch: snapshot
                .request()
                .transport_ref()
                .trim_start_matches("refs/heads/")
                .into(),
            workflow_path: workflow.path.clone(),
            event: "push".into(),
        };
        let metadata = bk::collect_ci_attested_validation(
            reader,
            pinned.build().clone(),
            &expectation,
            pinned.build_id(),
            &workflow.required_step_keys,
            &workflow.auxiliary_step_keys,
            budget,
        )?;
        if identity.repository != metadata.repository().full_name()
            && identity.repository != metadata.repository().https_url()
        {
            return Err(invalid("CI-attested repository identity mismatch"));
        }
        let required_jobs: Vec<_> = workflow
            .required_step_keys
            .iter()
            .chain(&workflow.auxiliary_step_keys)
            .map(|key| {
                Ok(evidence::BuildkiteRequiredJob {
                    step_key: key.clone(),
                    definition_digest: evidence::digest(&(&policy_digest, &workflow.path, key))
                        .map_err(io::Error::other)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let jobs: Vec<_> = metadata
            .jobs()
            .iter()
            .map(|(key, id)| evidence::BuildkiteJobObservation {
                step_key: key.clone(),
                job_id: id.clone(),
                conclusion: evidence::Conclusion::Success,
            })
            .collect();
        for obligation in project
            .catalog
            .obligations()
            .iter()
            .filter(|o| o.key.workflow == workflow.path)
        {
            let mappings: Vec<_> = workflow
                .checks
                .iter()
                .filter(|m| m.check == obligation.key)
                .collect();
            if mappings.len() != 1 {
                return Err(invalid(
                    "required validation check missing or ambiguous in accepted CI mapping",
                ));
            }
            let contract = evidence::BuildkiteCiContract {
                schema_version: evidence::BUILDKITE_CI_SCHEMA_VERSION.into(),
                material_digest: evidence::buildkite_ci_material_digest(identity)
                    .map_err(io::Error::other)?,
                intent_digest: intent_digest.clone(),
                key: obligation.key.clone(),
                event: expectation.event.clone(),
                source_definition_digest: obligation.definition_digest.clone(),
                executor_policy_digest: policy_digest.clone(),
                pipeline_id: expectation.pipeline_id.clone(),
                run_id: pinned.build_id().into(),
                required_jobs: required_jobs.clone(),
            };
            let binding = ExecutionBinding {
                role: obligation.role,
                definition_digest: obligation.definition_digest.clone(),
                invocation_digest: contract.digest().map_err(io::Error::other)?,
                toolchain_digest: policy_digest.clone(),
                executor: evidence::Provider::Buildkite,
                dependencies: dependencies(project, obligation)?,
            };
            let attestation = evidence::BuildkiteCiAttestation {
                contract,
                // Preserve the host's spelling only after verifying it against
                // the independently authenticated provider repository above.
                observed_repository: identity.repository.clone(),
                observed_commit: metadata.commit().into(),
                observed_branch: metadata.branch().into(),
                observed_pipeline_id: metadata.pipeline_id().into(),
                observed_workflow: metadata.workflow_path().into(),
                observed_event: metadata.event().into(),
                observed_run_id: metadata.run_id().into(),
                run_conclusion: evidence::Conclusion::Success,
                jobs: jobs.clone(),
                response_digest: metadata.response_digest().into(),
            };
            output.checks.insert(
                obligation.key.clone(),
                source_import::VerifiedCheck {
                    binding,
                    observation: evidence::Observation::CiAttested {
                        attestation: Box::new(attestation),
                    },
                },
            );
        }
    }
    if output.checks.len() != project.catalog.obligations().len() {
        return Err(invalid("incomplete CI-attested validation"));
    }
    Ok(output)
}
fn dependencies(
    project: &CurrentProject,
    obligation: &profile::Obligation,
) -> io::Result<Vec<evidence::Dependency>> {
    if obligation.kind == evidence::CheckKind::CandidateBuild {
        let setup = project
            .catalog
            .obligations()
            .iter()
            .find(|o| {
                o.kind == evidence::CheckKind::SourceCi
                    && o.key.workflow == obligation.key.workflow
                    && o.key.job == "setup"
            })
            .ok_or_else(|| invalid("candidate build requires configured setup validation"))?;
        return Ok(vec![evidence::Dependency::Check(setup.key.clone())]);
    }
    let id = obligation
        .key
        .profile
        .strip_prefix(&format!("{}/", profile::REQUIRED_PROFILE))
        .ok_or_else(|| invalid("unsupported validation profile"))?;
    source_import::source_dependencies(
        project,
        source::find_check(id)?,
        evidence::Provider::Buildkite,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> Policy {
        let path = ".github/workflows/ci.yml".to_string();
        Policy {
            schema: POLICY_SCHEMA.into(),
            workflows: vec![Workflow {
                path: path.clone(),
                required_step_keys: vec!["tests".into()],
                auxiliary_step_keys: vec!["importer".into()],
                checks: vec![CheckMapping {
                    check: CheckKey {
                        provider: evidence::Provider::Buildkite,
                        workflow: path,
                        job: "tests".into(),
                        matrix: BTreeMap::new(),
                        profile: "required".into(),
                    },
                    step_key: "tests".into(),
                }],
            }],
        }
    }
    #[test]
    fn only_identical_accepted_policy_is_admitted() {
        let accepted = policy();
        assert!(verify_policy_acceptance(&accepted, Some(&accepted), true).is_ok());
        assert!(verify_policy_acceptance(&accepted, Some(&accepted), false).is_err());
        assert!(verify_policy_acceptance(&accepted, None, true).is_err());
        let mut candidate = accepted.clone();
        candidate.workflows[0].auxiliary_step_keys.clear();
        assert!(verify_policy_acceptance(&candidate, Some(&accepted), true).is_err());
    }
    #[test]
    fn ambiguous_or_cross_workflow_mappings_are_rejected_even_if_accepted() {
        let mut policy = policy();
        let duplicate = policy.workflows[0].checks[0].clone();
        policy.workflows[0].checks.push(duplicate);
        assert!(verify_policy_acceptance(&policy, Some(&policy), true).is_err());
        policy.workflows[0].checks.pop();
        policy.workflows[0].checks[0].check.workflow = "other".into();
        assert!(verify_policy_acceptance(&policy, Some(&policy), true).is_err());
    }
    #[test]
    fn candidate_cannot_enable_or_change_acceptance_policy() {
        let policy = Policy {
            schema: POLICY_SCHEMA.into(),
            workflows: Vec::new(),
        };
        assert!(verify_policy_acceptance(&policy, None, true).is_err());
        assert!(verify_policy_acceptance(&policy, Some(&policy), false).is_err());
        assert!(verify_policy_acceptance(&policy, Some(&policy), true).is_err());
        let changed = Policy {
            schema: "unknown".into(),
            workflows: Vec::new(),
        };
        assert!(verify_policy_acceptance(&changed, Some(&policy), true).is_err());
    }
}
