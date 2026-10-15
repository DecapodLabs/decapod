use super::*;

fn hash(label: &str) -> String {
    digest(&label).unwrap()
}

fn fixture() -> (
    ExpectedPlan,
    MaterialIdentity,
    ApplicabilityFacts,
    SuiteEvidence,
) {
    let identity = MaterialIdentity {
        repository: "github.com/DecapodLabs/decapod".into(),
        work_branch: "work/ci-attested".into(),
        head_revision: "a".repeat(40),
        base_branch: "master".into(),
        accepted_base_revision: "b".repeat(40),
        history_digest: hash("history"),
        tree_digest: hash("tree"),
        configuration_digest: hash("accepted configuration"),
        workflow_digest: hash("source workflows"),
        toolchain_digest: hash("aggregate policy bindings"),
        candidate_digest: hash("candidate inputs"),
    };
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: ["src/lib.rs".into()].into(),
    };
    let mut plan = ExpectedPlan {
        schema_version: SCHEMA_VERSION.into(),
        policy_digest: hash("independently accepted CI trust policy"),
        source_inventory_digest: hash("complete source inventory"),
        buildkite_enabled: true,
        checks: Vec::new(),
    };
    let mut results = Vec::new();
    for (index, kind) in [CheckKind::SourceCi, CheckKind::CandidateBuild]
        .into_iter()
        .enumerate()
    {
        let key = CheckKey {
            provider: Provider::Buildkite,
            workflow: ".github/workflows/ci.yml".into(),
            job: format!("logical-job-{index}"),
            matrix: [("os".into(), "ubuntu-24.04".into())].into(),
            profile: format!("source-check-{index}"),
        };
        let definition_digest = hash(&format!("source-definition-{index}"));
        let executor_policy_digest = hash("accepted executor policy, not measured tools");
        let contract = BuildkiteCiContract {
            schema_version: BUILDKITE_CI_SCHEMA_VERSION.into(),
            material_digest: buildkite_ci_material_digest(&identity).unwrap(),
            intent_digest: hash("current governed intent"),
            key: key.clone(),
            event: "push".into(),
            source_definition_digest: definition_digest.clone(),
            executor_policy_digest: executor_policy_digest.clone(),
            pipeline_id: "11111111-1111-4111-8111-111111111111".into(),
            run_id: "22222222-2222-4222-8222-222222222222".into(),
            required_jobs: ["native-test", "native-build", "accepted-bootstrap"]
                .into_iter()
                .map(|step_key| BuildkiteRequiredJob {
                    step_key: step_key.into(),
                    definition_digest: hash(step_key),
                })
                .collect(),
        };
        let invocation_digest = contract.digest().unwrap();
        let attestation = BuildkiteCiAttestation {
            observed_repository: identity.repository.clone(),
            observed_commit: identity.head_revision.clone(),
            observed_branch: identity.work_branch.clone(),
            observed_pipeline_id: contract.pipeline_id.clone(),
            observed_workflow: key.workflow.clone(),
            observed_event: contract.event.clone(),
            observed_run_id: contract.run_id.clone(),
            run_conclusion: Conclusion::Success,
            jobs: contract
                .required_jobs
                .iter()
                .enumerate()
                .map(|(index, job)| BuildkiteJobObservation {
                    step_key: job.step_key.clone(),
                    job_id: format!("33333333-3333-4333-8333-{index:012}"),
                    conclusion: Conclusion::Success,
                })
                .collect(),
            response_digest: hash("actual provider response bytes"),
            contract,
        };
        plan.checks.push(ExpectedCheck {
            key: key.clone(),
            kind,
            definition_digest: definition_digest.clone(),
            invocation_digest: invocation_digest.clone(),
            toolchain_digest: executor_policy_digest.clone(),
            executor: Provider::Buildkite,
            dependencies: Vec::new(),
            requirement: Requirement::Execute,
        });
        results.push(CheckResult {
            key,
            definition_digest,
            invocation_digest,
            toolchain_digest: executor_policy_digest,
            executor: Provider::Buildkite,
            identity: identity.clone(),
            observation: Observation::CiAttested {
                attestation: Box::new(attestation),
            },
        });
    }
    let evidence = SuiteEvidence {
        schema_version: SCHEMA_VERSION.into(),
        expected_plan_digest: plan.digest().unwrap(),
        before: identity.clone(),
        after: identity.clone(),
        facts: facts.clone(),
        results,
    };
    (plan, identity, facts, evidence)
}

fn attestation(evidence: &mut SuiteEvidence) -> &mut BuildkiteCiAttestation {
    let Observation::CiAttested { attestation } = &mut evidence.results[0].observation else {
        panic!("CI-attested fixture required");
    };
    attestation
}

fn not_ready(
    plan: &ExpectedPlan,
    current: &MaterialIdentity,
    facts: &ApplicabilityFacts,
    evidence: &SuiteEvidence,
) {
    let result = verify(plan, current, facts, evidence);
    assert_eq!(result.verdict, Verdict::NotReady, "{result:?}");
    assert!(!result.failures.is_empty());
}

fn rebind_contract(plan: &mut ExpectedPlan, evidence: &mut SuiteEvidence) {
    let digest = attestation(evidence).contract.digest().unwrap();
    plan.checks[0].invocation_digest = digest.clone();
    evidence.results[0].invocation_digest = digest;
    evidence.expected_plan_digest = plan.digest().unwrap();
}

#[test]
fn complete_ci_attested_suite_proves_execution_without_inventing_measurements() {
    let (plan, identity, facts, evidence) = fixture();
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
    let json = serde_json::to_value(&evidence).unwrap();
    for row in json["results"].as_array().unwrap() {
        assert_eq!(row["observation"]["kind"], "ci_attested");
        let observed = row["observation"]["attestation"].as_object().unwrap();
        for absent in [
            "exit_code",
            "output_digest",
            "artifact_digest",
            "log_digest",
            "run_attempt",
        ] {
            assert!(!observed.contains_key(absent), "invented {absent}");
        }
    }
    assert_eq!(
        serde_json::from_value::<SuiteEvidence>(json).unwrap(),
        evidence
    );
}

#[test]
fn every_required_row_including_candidate_build_remains_mandatory() {
    for index in 0..2 {
        let (plan, identity, facts, mut evidence) = fixture();
        evidence.results.remove(index);
        not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn pending_running_failed_skipped_cancelled_timed_out_run_or_job_never_passes() {
    for state in [
        Conclusion::Pending,
        Conclusion::Running,
        Conclusion::Failure,
        Conclusion::Skipped,
        Conclusion::Cancelled,
        Conclusion::TimedOut,
    ] {
        for subject in 0..4 {
            let (plan, identity, facts, mut evidence) = fixture();
            let attestation = attestation(&mut evidence);
            if subject == 0 {
                attestation.run_conclusion = state;
            } else {
                attestation.jobs[subject - 1].conclusion = state;
            }
            not_ready(&plan, &identity, &facts, &evidence);
        }
    }
}

#[test]
fn incomplete_extra_duplicate_or_wrong_native_job_sets_fail() {
    for case in 0..7 {
        let (plan, identity, facts, mut evidence) = fixture();
        let attestation = attestation(&mut evidence);
        match case {
            0 => {
                attestation.jobs.pop();
            }
            1 => attestation.jobs.push(BuildkiteJobObservation {
                step_key: "unexpected-job".into(),
                job_id: "44444444-4444-4444-8444-444444444444".into(),
                conclusion: Conclusion::Success,
            }),
            2 => attestation.jobs[1] = attestation.jobs[0].clone(),
            3 => attestation.jobs[1].job_id = attestation.jobs[0].job_id.clone(),
            4 => attestation.jobs[0].step_key = "wrong-matrix-job".into(),
            5 => attestation.jobs[0].job_id.clear(),
            _ => attestation.jobs[0].job_id = "00000000-0000-0000-0000-000000000000".into(),
        }
        not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn job_order_does_not_change_the_complete_required_set() {
    let (plan, identity, facts, mut evidence) = fixture();
    let attestation = attestation(&mut evidence);
    attestation.jobs.reverse();
    attestation.contract.required_jobs.reverse();
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
}

#[test]
fn evidence_cannot_rewrite_intent_policy_job_definitions_or_required_coverage() {
    let changes: [fn(&mut BuildkiteCiContract); 9] = [
        |contract| contract.intent_digest = hash("different intent"),
        |contract| contract.executor_policy_digest = hash("candidate policy"),
        |contract| contract.material_digest = hash("candidate config changed"),
        |contract| contract.source_definition_digest = hash("different source check"),
        |contract| contract.required_jobs[0].definition_digest = hash("weaker job"),
        |contract| {
            contract.required_jobs.pop();
        },
        |contract| contract.event = "scheduled".into(),
        |contract| contract.run_id = "44444444-4444-4444-8444-444444444444".into(),
        |contract| contract.pipeline_id = "44444444-4444-4444-8444-444444444444".into(),
    ];
    for change in changes {
        let (plan, identity, facts, mut evidence) = fixture();
        change(&mut attestation(&mut evidence).contract);
        // The independently expected invocation remains unchanged. Observed
        // payloads cannot grant policy or redefine what was required to pass.
        not_ready(&plan, &identity, &facts, &evidence);
    }
    let (plan, identity, facts, mut evidence) = fixture();
    let attestation = attestation(&mut evidence);
    attestation.contract.required_jobs.pop();
    attestation.jobs.pop();
    not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn wrong_provider_repository_commit_branch_pipeline_workflow_event_or_run_fails() {
    let changes: [fn(&mut BuildkiteCiAttestation); 9] = [
        |a| a.observed_repository = "github.com/untrusted/repository".into(),
        |a| a.observed_commit = "c".repeat(40),
        |a| a.observed_branch = "other-branch".into(),
        |a| a.observed_pipeline_id = "44444444-4444-4444-8444-444444444444".into(),
        |a| a.observed_workflow = ".github/workflows/other.yml".into(),
        |a| a.observed_event = "pull_request".into(),
        |a| a.observed_run_id = "44444444-4444-4444-8444-444444444444".into(),
        |a| a.response_digest.clear(),
        |a| a.contract.key.provider = Provider::GithubActions,
    ];
    for change in changes {
        let (plan, identity, facts, mut evidence) = fixture();
        change(attestation(&mut evidence));
        not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn malformed_or_unbound_contracts_fail_even_if_the_expected_digest_is_rebound() {
    let changes: [fn(&mut BuildkiteCiAttestation); 12] = [
        |a| a.contract.schema_version = "unknown".into(),
        |a| a.contract.intent_digest.clear(),
        |a| a.contract.event.clear(),
        |a| a.contract.run_id.clear(),
        |a| a.contract.pipeline_id = "00000000-0000-0000-0000-000000000000".into(),
        |a| a.contract.key.job = "other-source-job".into(),
        |a| a.contract.source_definition_digest = hash("other-source-definition"),
        |a| a.contract.executor_policy_digest = hash("other-executor-policy"),
        |a| a.contract.required_jobs[0].definition_digest.clear(),
        |a| a.contract.required_jobs[0].step_key.clear(),
        |a| {
            a.contract.required_jobs.clear();
            a.jobs.clear();
        },
        |a| {
            a.contract.required_jobs[1] = a.contract.required_jobs[0].clone();
            a.jobs[1] = a.jobs[0].clone();
        },
    ];
    for change in changes {
        let (mut plan, identity, facts, mut evidence) = fixture();
        change(attestation(&mut evidence));
        rebind_contract(&mut plan, &mut evidence);
        not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn stale_source_intent_configuration_or_material_cannot_reuse_a_ci_pass() {
    let changes: [fn(&mut MaterialIdentity); 11] = [
        |i| i.repository = "github.com/other/repository".into(),
        |i| i.work_branch = "another-work-branch".into(),
        |i| i.head_revision = "c".repeat(40),
        |i| i.base_branch = "another-base".into(),
        |i| i.accepted_base_revision = "d".repeat(40),
        |i| i.history_digest = hash("other-history"),
        |i| i.tree_digest = hash("other-tree"),
        |i| i.configuration_digest = hash("other-configuration"),
        |i| i.workflow_digest = hash("other-workflow"),
        |i| i.toolchain_digest = hash("other-policy-bindings"),
        |i| i.candidate_digest = hash("other-candidate"),
    ];
    for change in changes {
        let (plan, mut identity, facts, evidence) = fixture();
        change(&mut identity);
        not_ready(&plan, &identity, &facts, &evidence);
    }
    let (plan, mut identity, facts, mut evidence) = fixture();
    identity.configuration_digest = hash("new configuration");
    evidence.before = identity.clone();
    evidence.after = identity.clone();
    for result in &mut evidence.results {
        result.identity = identity.clone();
    }
    // Rewriting the envelope cannot detach a CI contract from its original source.
    not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn ci_attestation_cannot_authorize_another_provider_or_repository_local_execution() {
    for executor in [Provider::Native, Provider::GithubActions] {
        let (mut plan, identity, facts, mut evidence) = fixture();
        plan.checks[0].executor = executor;
        evidence.results[0].executor = executor;
        evidence.expected_plan_digest = plan.digest().unwrap();
        not_ready(&plan, &identity, &facts, &evidence);
    }
    let (mut plan, identity, facts, mut evidence) = fixture();
    plan.buildkite_enabled = false;
    evidence.expected_plan_digest = plan.digest().unwrap();
    not_ready(&plan, &identity, &facts, &evidence);
    for kind in [
        CheckKind::RepositoryLocal,
        CheckKind::DerivedRelease,
        CheckKind::Validator,
    ] {
        let (mut plan, identity, facts, mut evidence) = fixture();
        plan.checks[0].kind = kind;
        evidence.expected_plan_digest = plan.digest().unwrap();
        not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn ci_attestation_deserialization_rejects_fabricated_measurement_fields() {
    let (_, _, _, evidence) = fixture();
    for field in [
        "exit_code",
        "output_digest",
        "artifact_digest",
        "log_digest",
    ] {
        let mut json = serde_json::to_value(&evidence).unwrap();
        json["results"][0]["observation"]["attestation"][field] = serde_json::json!("fabricated");
        assert!(serde_json::from_value::<SuiteEvidence>(json).is_err());
    }
}
