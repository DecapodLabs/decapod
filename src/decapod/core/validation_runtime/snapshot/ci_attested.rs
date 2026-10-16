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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_trigger: Option<bk::PipelineTriggerPolicy>,
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
        if let Some(trigger) = &workflow.pipeline_trigger {
            trigger.validate()?;
            supported_pipeline_workflow(&workflow.path)?;
        }
        for key in workflow
            .required_step_keys
            .iter()
            .chain(&workflow.auxiliary_step_keys)
        {
            if key.is_empty()
                || !keys.insert(key)
                || (workflow.pipeline_trigger.is_some()
                    && (key == bk::PIPELINE_TRIGGER_IDENTITY
                        || key.starts_with("gha-concurrency-")))
            {
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

fn supported_pipeline_workflow(path: &str) -> io::Result<()> {
    if !matches!(
        path,
        ".github/workflows/ci.yml"
            | ".github/workflows/constitution-sync.yml"
            | ".github/workflows/decapod-validate.yml"
            | ".github/workflows/docs.yml"
            | ".github/workflows/docs_sync.yml"
    ) {
        return Err(invalid("unsupported CI-attested Pipeline Trigger workflow"));
    }
    Ok(())
}

/// This finite source shape is deliberately not a YAML/expression interpreter.
/// Unsupported quoting, aliases, merge keys, duplicate fields and alternate
/// concurrency expressions fail closed rather than guessing imported identities.
fn verify_workflow_source(policy: &bk::PipelineTriggerPolicy, source: &str) -> io::Result<()> {
    policy.validate()?;
    let mut fields = BTreeSet::new();
    let mut section = "";
    let mut name = None;
    let mut concurrency = Vec::new();
    for line in source.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(' ') {
            if section == "concurrency" {
                concurrency.push(line);
            }
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("unsupported CI-attested workflow source shape"))?;
        if !matches!(
            key,
            "name"
                | "on"
                | "permissions"
                | "concurrency"
                | "env"
                | "defaults"
                | "jobs"
                | "run-name"
        ) || !fields.insert(key)
        {
            return Err(invalid(
                "unsupported or duplicate CI-attested workflow source field",
            ));
        }
        section = key;
        match key {
            "name" => name = Some(value.trim()),
            "concurrency" if !value.trim().is_empty() => {
                return Err(invalid("unsupported CI-attested concurrency source shape"));
            }
            _ => {}
        }
    }
    if name != Some(policy.workflow_name.as_str())
        || !fields.contains("jobs")
        || !fields.contains("on")
    {
        return Err(invalid(
            "CI-attested accepted workflow name/source mismatch",
        ));
    }
    match &policy.concurrency_group {
        None if !fields.contains("concurrency") => Ok(()),
        Some(group)
            if fields.contains("concurrency")
                && concurrency
                    == [
                        format!("  group: {group}"),
                        "  cancel-in-progress: true".into(),
                    ] =>
        {
            Ok(())
        }
        _ => Err(invalid("CI-attested accepted concurrency source mismatch")),
    }
}

fn selected_workflows<'a>(
    policy: &'a Policy,
    required: &BTreeSet<&str>,
) -> io::Result<Vec<&'a Workflow>> {
    let declared: BTreeSet<_> = policy.workflows.iter().map(|w| w.path.as_str()).collect();
    if declared.len() != policy.workflows.len() || !required.is_subset(&declared) {
        return Err(invalid(
            "CI-attested policy misses or duplicates a required workflow",
        ));
    }
    for workflow in &policy.workflows {
        if workflow.pipeline_trigger.is_some() {
            supported_pipeline_workflow(&workflow.path)?;
        }
    }
    Ok(policy
        .workflows
        .iter()
        .filter(|w| required.contains(w.path.as_str()))
        .collect())
}

fn verify_declared_sources(project: &CurrentProject, policy: &Policy) -> io::Result<()> {
    let known_checks: BTreeSet<_> = project
        .catalog
        .obligations()
        .iter()
        .map(|o| &o.key)
        .chain(project.catalog.exclusions().iter().map(|e| &e.key))
        .collect();
    for obligation in project.catalog.obligations() {
        if policy
            .workflows
            .iter()
            .flat_map(|w| &w.checks)
            .filter(|m| m.check == obligation.key)
            .count()
            != 1
        {
            return Err(invalid(
                "required validation check missing or ambiguous in accepted CI mapping",
            ));
        }
    }
    for workflow in &policy.workflows {
        if workflow
            .checks
            .iter()
            .any(|m| !known_checks.contains(&m.check))
        {
            return Err(invalid(
                "CI-attested mapping is outside the independently compiled catalog",
            ));
        }
        if let Some(trigger) = &workflow.pipeline_trigger {
            supported_pipeline_workflow(&workflow.path)?;
            let path = file(&project.identity_inputs.root, &workflow.path)?;
            let mut options = fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let source = options.open(path)?;
            if !source.metadata()?.is_file() {
                return Err(invalid("CI-attested workflow is not a regular source file"));
            }
            let mut bytes = Vec::new();
            source.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024
                || project.sources.files.get(&workflow.path)
                    != Some(&format!("sha256:{:x}", Sha256::digest(&bytes)))
            {
                return Err(invalid(
                    "CI-attested workflow differs from reviewed source inventory",
                ));
            }
            verify_workflow_source(
                trigger,
                std::str::from_utf8(&bytes).map_err(io::Error::other)?,
            )?;
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
    let workflows = selected_workflows(policy, &expected_workflows)?;
    verify_declared_sources(project, policy)?;
    for workflow in workflows {
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
        let metadata = bk::collect_ci_attested_validation_with_pipeline_trigger(
            reader,
            pinned.build().clone(),
            &expectation,
            pinned.build_id(),
            &workflow.required_step_keys,
            &workflow.auxiliary_step_keys,
            workflow.pipeline_trigger.as_ref(),
            budget,
        )?;
        if identity.repository != metadata.repository().full_name()
            && identity.repository != metadata.repository().https_url()
        {
            return Err(invalid("CI-attested repository identity mismatch"));
        }
        // Include derived gate identities and the explicitly typed bootstrap
        // bookkeeping slot; observations keep every actual provider job UUID.
        let mut required_keys = workflow.required_step_keys.clone();
        required_keys.extend(workflow.auxiliary_step_keys.clone());
        if let Some(trigger) = &workflow.pipeline_trigger {
            required_keys.extend(trigger.auxiliary_step_keys(&expectation)?);
        }
        let required_jobs: Vec<_> = required_keys
            .iter()
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
                pipeline_trigger: None,
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
    fn trigger() -> bk::PipelineTriggerPolicy {
        bk::PipelineTriggerPolicy {
            contract: bk::PipelineTriggerContract::BuildkiteGha01027,
            workflow_name: "CI".into(),
            concurrency_group: Some(bk::WORKFLOW_REF_CONCURRENCY.into()),
        }
    }
    #[test]
    fn pipeline_trigger_policy_is_explicit_and_exactly_accepted() {
        let legacy = policy();
        let legacy_json = serde_json::to_value(&legacy).unwrap();
        assert!(
            legacy_json["workflows"][0]
                .get("pipeline_trigger")
                .is_none()
        );
        assert_eq!(
            serde_json::from_value::<Policy>(legacy_json).unwrap(),
            legacy
        );
        let mut candidate = legacy.clone();
        candidate.workflows[0].pipeline_trigger = Some(trigger());
        assert!(verify_policy_acceptance(&candidate, Some(&legacy), true).is_err());
        assert!(verify_policy_acceptance(&candidate, Some(&candidate), true).is_ok());
        let mut unknown = serde_json::to_value(&candidate).unwrap();
        unknown["workflows"][0]["pipeline_trigger"]["contract"] = "infer-from-green-jobs".into();
        assert!(serde_json::from_value::<Policy>(unknown).is_err());
        candidate.workflows[0]
            .auxiliary_step_keys
            .push(bk::PIPELINE_TRIGGER_IDENTITY.into());
        assert!(verify_policy_acceptance(&candidate, Some(&candidate), true).is_err());
    }
    #[test]
    fn pipeline_trigger_binds_accepted_workflow_name_and_exact_supported_concurrency_source() {
        let source = format!(
            "name: CI\non:\n  push:\njobs:\n  tests:\nconcurrency:\n  group: {}\n  cancel-in-progress: true\n",
            bk::WORKFLOW_REF_CONCURRENCY
        );
        assert!(verify_workflow_source(&trigger(), &source).is_ok());
        for changed in [
            source.replace("name: CI", "name: Other"),
            source.replace("name: CI", "name: CI\nname: CI"),
            source.replace("github.workflow", "github.sha"),
            source.replace("cancel-in-progress: true", "cancel-in-progress: false"),
            source.replace("concurrency:", "concurrency: &group"),
            source.replace("concurrency:", "concurrency:\n  unknown: true"),
            format!("{source}<<: *another-workflow\n"),
        ] {
            assert!(verify_workflow_source(&trigger(), &changed).is_err());
        }
        let mut no_concurrency = trigger();
        no_concurrency.concurrency_group = None;
        assert!(
            verify_workflow_source(&no_concurrency, "name: CI\non:\n  push:\njobs:\n  tests:\n")
                .is_ok()
        );
        assert!(verify_workflow_source(&no_concurrency, &source).is_err());
        assert!(
            verify_workflow_source(&trigger(), "name: CI\non:\n  push:\njobs:\n  tests:\n")
                .is_err()
        );
    }
    #[test]
    fn configured_inactive_workflow_does_not_require_a_run_but_missing_or_unknown_workflows_fail() {
        let mut policy = policy();
        policy.workflows[0].pipeline_trigger = Some(trigger());
        let mut docs = policy.workflows[0].clone();
        docs.path = ".github/workflows/docs_sync.yml".into();
        docs.pipeline_trigger.as_mut().unwrap().concurrency_group = None;
        docs.pipeline_trigger.as_mut().unwrap().workflow_name = "Documentation Sync".into();
        policy.workflows.push(docs);
        let required = BTreeSet::from([".github/workflows/ci.yml"]);
        let selected = selected_workflows(&policy, &required).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path, ".github/workflows/ci.yml");
        assert!(
            selected_workflows(&policy, &BTreeSet::from([".github/workflows/docs.yml"])).is_err()
        );
        policy.workflows[1].path = ".github/workflows/unreviewed.yml".into();
        assert!(selected_workflows(&policy, &required).is_err());
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
