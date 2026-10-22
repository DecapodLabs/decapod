//! Regression captures are minimized provider metadata, not raw responses or
//! execution custody. See fixtures/README.md for sources and transformations.
use super::*;
use crate::core::readiness_evidence::{CheckKey, Provider};
use crate::core::readiness_profile::{REQUIRED_PROFILE, source};
use serde::Deserialize;
use serde_json::json;

const WORKFLOW: &str = ".github/workflows/ci.yml";
const PIPELINE: &str = "01a0bcf1-5e16-44ae-8457-04495fae8ff5";
const BOOTSTRAP: &str = "buildkite-pipeline-trigger";
const CONCURRENCY: &str =
    "${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}";

struct Capture {
    number: u64,
    run: &'static str,
    commit: &'static str,
    branch: &'static str,
    bodies: [&'static [u8]; 3],
}
const PASSED: Capture = Capture {
    number: 1840,
    run: "01a14183-310b-4426-8bac-b391137102d4",
    commit: "c444ff9b03ae569a268f348c898f0dccfa08b5e4",
    branch: "agent/laya-readiness/cicd_01m50ns7bafppa7k",
    bodies: [
        include_bytes!("fixtures/1840-envelope.json"),
        include_bytes!("fixtures/1840-steps.json"),
        include_bytes!("fixtures/1840-jobs.json"),
    ],
};
const FAILED: Capture = Capture {
    number: 1870,
    run: "01a14205-d31a-4ae0-9fb2-c92b61248f00",
    commit: "bcb5842ff5d1dba6a8697b1cffe5deb1d135f701",
    branch: "agent/laya-readiness/code_01m50zemd9zw1swe",
    bodies: [
        include_bytes!("fixtures/1870-envelope.json"),
        include_bytes!("fixtures/1870-steps.json"),
        include_bytes!("fixtures/1870-jobs.json"),
    ],
};
impl Capture {
    fn build(&self) -> PublicBuild {
        PublicBuild::new("decapodlabs", "decapod", self.number).unwrap()
    }
    fn expectation(&self) -> InventoryExpectation {
        InventoryExpectation {
            repository: Repository::new("DecapodLabs", "decapod").unwrap(),
            pipeline_id: PIPELINE.into(),
            commit: self.commit.into(),
            branch: self.branch.into(),
            workflow_path: WORKFLOW.into(),
            event: "push".into(),
        }
    }
    fn values(&self) -> [Value; 3] {
        self.bodies
            .map(|body| serde_json::from_slice(body).unwrap())
    }
    fn reader(&self) -> MetadataOnlyReader {
        MetadataOnlyReader::new(self, self.bodies.map(<[u8]>::to_vec))
    }
}

/// The allowlist is exact, including the query requiring retried jobs. Any log,
/// artifact, redirect, different build, repeated request or fourth read fails.
struct MetadataOnlyReader {
    bodies: [Vec<u8>; 3],
    urls: [String; 3],
    seen: Vec<String>,
}
impl MetadataOnlyReader {
    fn new(capture: &Capture, bodies: [Vec<u8>; 3]) -> Self {
        let base = format!(
            "https://buildkite.com/decapodlabs/decapod/builds/{}",
            capture.number
        );
        Self {
            bodies,
            urls: [
                format!("{base}.json"),
                format!("{base}/data/steps"),
                format!("{base}/data/jobs?include_retried_jobs=true&paginate=false"),
            ],
            seen: Vec::new(),
        }
    }
    fn assert_exact_reads(&self) {
        assert_eq!(self.seen, self.urls);
    }
}
impl PublicReader for MetadataOnlyReader {
    fn get(&mut self, request: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        let read = request.authorize_read()?;
        assert_eq!(read.method(), "GET");
        assert_eq!(read.max_https_redirects(), 0);
        let index = match read.kind() {
            PublicReadKind::BuildEnvelope => 0,
            PublicReadKind::FullSteps => 1,
            PublicReadKind::FullJobs => 2,
            PublicReadKind::JobArtifacts
            | PublicReadKind::ArtifactContent
            | PublicReadKind::JobLog => {
                panic!("CI-attested regression must never read artifacts or logs")
            }
        };
        assert_eq!(read.url(), self.urls[index]);
        assert!(!self.seen.iter().any(|url| url == read.url()));
        assert!(self.seen.len() < 3);
        self.seen.push(read.url().into());
        Ok(self.bodies[index].clone())
    }
}

#[derive(Deserialize)]
struct AcceptedJob {
    workflow: String,
    job: String,
    matrix: BTreeMap<String, String>,
    checks: Vec<String>,
    step_key: String,
}
fn accepted_mapping() -> Vec<AcceptedJob> {
    // Authored workflow/catalog inputs, frozen before comparing provider rows.
    serde_json::from_slice(include_bytes!(
        "fixtures/accepted-workflow-key-mapping.json"
    ))
    .unwrap()
}
fn required() -> Vec<String> {
    let mut keys: Vec<_> = accepted_mapping()
        .into_iter()
        .filter(|row| row.workflow == WORKFLOW)
        .map(|row| row.step_key)
        .collect();
    keys.sort();
    keys
}
fn policy() -> PipelineTriggerPolicy {
    PipelineTriggerPolicy {
        contract: PipelineTriggerContract::BuildkiteGha01027,
        workflow_name: "CI".into(),
        concurrency_group: Some(CONCURRENCY.into()),
    }
}
fn collect(
    reader: &mut MetadataOnlyReader,
    capture: &Capture,
    expected: &InventoryExpectation,
) -> Result<CiAttestedValidation> {
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(2), 1024 * 1024, &cancel).unwrap();
    collect_ci_attested_validation_with_pipeline_trigger(
        reader,
        capture.build(),
        expected,
        capture.run,
        &required(),
        &[],
        Some(&policy()),
        &budget,
    )
}
fn collect_values(capture: &Capture, values: [Value; 3]) -> Result<CiAttestedValidation> {
    let mut reader = MetadataOnlyReader::new(
        capture,
        values.map(|value| serde_json::to_vec(&value).unwrap()),
    );
    let result = collect(&mut reader, capture, &capture.expectation());
    reader.assert_exact_reads();
    result
}
fn pair(values: &[Value; 3], key: Option<&str>) -> (usize, usize) {
    let step = values[1]
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row["key"].as_str() == key)
        .unwrap();
    let job = values[2]["records"]
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row["step_key"].as_str() == key)
        .unwrap();
    (step, job)
}
fn remove_pair(values: &mut [Value; 3], key: &str) {
    let (step, job) = pair(values, Some(key));
    values[1].as_array_mut().unwrap().remove(step);
    values[2]["records"].as_array_mut().unwrap().remove(job);
    update_counts(values);
}
fn update_counts(values: &mut [Value; 3]) {
    values[0]["statistics"]["steps_count"] = json!(values[1].as_array().unwrap().len());
    values[0]["statistics"]["jobs_count"] = json!(values[2]["records"].as_array().unwrap().len());
}
fn duplicate_pair(values: &mut [Value; 3], key: Option<&str>) {
    let (step, job) = pair(values, key);
    let mut step = values[1][step].clone();
    let mut job = values[2]["records"][job].clone();
    let step_id = "11111111-2222-4333-8444-555555555555";
    let job_id = "66666666-7777-4888-8999-000000000000";
    step["uuid"] = json!(step_id);
    step["statistics"]["latest_job_id"] = json!(job_id);
    job["id"] = json!(job_id);
    job["step_uuid"] = json!(step_id);
    job["base_path"] = json!(PASSED.build().job_base(job_id).unwrap());
    values[1].as_array_mut().unwrap().push(step);
    values[2]["records"].as_array_mut().unwrap().push(job);
    update_counts(values);
}

#[test]
fn real_1840_attests_all_39_successful_native_jobs_with_only_three_metadata_gets() {
    let mut reader = PASSED.reader();
    let result = collect(&mut reader, &PASSED, &PASSED.expectation()).unwrap();
    reader.assert_exact_reads();
    assert_eq!(result.run_id(), PASSED.run);
    assert_eq!(result.commit(), PASSED.commit);
    assert_eq!(result.branch(), PASSED.branch);
    assert_eq!(result.pipeline_id(), PIPELINE);
    assert_eq!(result.jobs().len(), 39);
    assert_eq!(result.required_jobs().len(), 36);
    assert_eq!(result.auxiliary_jobs().len(), 3);
    assert_eq!(
        result.auxiliary_jobs()[BOOTSTRAP],
        "01a14183-3110-43bc-a138-eb9f55a9d4fc"
    );
    assert_eq!(
        result.auxiliary_jobs()["gha-concurrency-start-5a0d85d531e3"],
        "01a14186-226a-453a-ac42-ddc52542dd7b"
    );
    assert_eq!(
        result.required_jobs().keys().cloned().collect::<Vec<_>>(),
        required()
    );
    assert_eq!(result.response_digests(), &PASSED.bodies.map(hash));
    assert_eq!(result.metadata_digest(), metadata_digest(PASSED.bodies));
}

#[test]
fn real_1870_failure_cannot_be_attested_even_with_a_green_aggregate_mutation() {
    let mut reader = FAILED.reader();
    assert!(collect(&mut reader, &FAILED, &FAILED.expectation()).is_err());
    reader.assert_exact_reads();
    let mut values = FAILED.values();
    let failed: Vec<_> = values[2]["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["passed"] == false)
        .map(|row| row["step_key"].as_str().unwrap())
        .collect();
    assert_eq!(
        failed,
        vec![
            "gha-b803fcb7f17ed923-test-integration-f5ffb834e199",
            "gha-b803fcb7f17ed923-integration-status",
        ]
    );
    values[0]["state"] = json!("passed");
    assert!(collect_values(&FAILED, values).is_err());
}

#[test]
fn real_metadata_does_not_enable_keyless_bootstrap_without_accepted_policy() {
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(2), 1024 * 1024, &cancel).unwrap();
    let mut reader = PASSED.reader();
    assert!(
        collect_ci_attested_validation(
            &mut reader,
            PASSED.build(),
            &PASSED.expectation(),
            PASSED.run,
            &required(),
            &[],
            &budget,
        )
        .is_err()
    );
    reader.assert_exact_reads();
}

#[test]
fn real_capture_rejects_wrong_commit_branch_workflow_pipeline_repository_and_run() {
    for case in 0..6 {
        let mut values = PASSED.values();
        match case {
            0 => values[0]["commit_id"] = json!(FAILED.commit),
            1 => values[0]["branch_name"] = json!(FAILED.branch),
            2 => values[0]["github_workflow_path"] = json!(".github/workflows/docs.yml"),
            3 => values[0]["project"]["id"] = json!(FAILED.run),
            4 => {
                values[0]["project"]["provider"]["url"] = json!("https://github.com/other/decapod")
            }
            5 => values[0]["id"] = json!(FAILED.run),
            _ => unreachable!(),
        }
        assert!(
            collect_values(&PASSED, values).is_err(),
            "identity mutation {case}"
        );
    }
}

#[test]
fn independently_required_jobs_and_exact_auxiliaries_cannot_shrink_or_expand() {
    for case in 0..5 {
        let mut values = PASSED.values();
        match case {
            0 => remove_pair(&mut values, "gha-b803fcb7f17ed923-fmt"),
            1 => remove_pair(&mut values, "gha-concurrency-start-5a0d85d531e3"),
            2 => duplicate_pair(&mut values, None),
            3 => duplicate_pair(&mut values, Some("gha-concurrency-finish-5a0d85d531e3")),
            4 => {
                let (step, job) = pair(&values, Some("gha-b803fcb7f17ed923-fmt"));
                values[1][step]["key"] = json!("unknown-successful-job");
                values[2]["records"][job]["step_key"] = json!("unknown-successful-job");
            }
            _ => unreachable!(),
        }
        assert!(
            collect_values(&PASSED, values).is_err(),
            "job-set mutation {case}"
        );
    }
}

#[test]
fn real_bootstrap_failure_bad_parentage_and_retries_are_never_auxiliary_exemptions() {
    for case in 0..6 {
        let mut values = PASSED.values();
        let (root_step, root_job) = pair(&values, None);
        let (check_step, check_job) = pair(&values, Some("gha-b803fcb7f17ed923-fmt"));
        match case {
            0 => {
                values[2]["records"][root_job]["passed"] = json!(false);
                values[2]["records"][root_job]["exit_status"] = json!(1);
            }
            1 => values[1][root_step]["statistics"]["retries_count"] = json!(1),
            2 => values[2]["records"][check_job]["retry_pending"] = json!(true),
            3 => values[2]["records"][root_job]["retried_at"] = json!("2026-10-15T22:01:00Z"),
            4 => values[1][check_step]["uploaded_by_step_uuid"] = Value::Null,
            5 => {
                values[1][root_step]["uploaded_by_step_uuid"] =
                    values[1][check_step]["uuid"].clone()
            }
            _ => unreachable!(),
        }
        assert!(
            collect_values(&PASSED, values).is_err(),
            "bootstrap/retry mutation {case}"
        );
    }
}

#[test]
fn branch_changes_rederive_concurrency_keys_and_cannot_reuse_successful_old_gates() {
    let original = PASSED.expectation();
    let changed = FAILED.expectation();
    let first = policy().auxiliary_step_keys(&original).unwrap();
    let second = policy().auxiliary_step_keys(&changed).unwrap();
    assert_eq!(
        first.into_iter().collect::<BTreeSet<_>>(),
        [
            BOOTSTRAP.to_string(),
            "gha-concurrency-start-5a0d85d531e3".into(),
            "gha-concurrency-finish-5a0d85d531e3".into(),
        ]
        .into()
    );
    assert_eq!(
        second.into_iter().collect::<BTreeSet<_>>(),
        [
            BOOTSTRAP.to_string(),
            "gha-concurrency-start-6bb3eb42f9e9".into(),
            "gha-concurrency-finish-6bb3eb42f9e9".into(),
        ]
        .into()
    );
    // Rebind the envelope consistently, leaving only the old branch's gates.
    let mut values = PASSED.values();
    let mut expected = PASSED.expectation();
    expected.branch = FAILED.branch.into();
    values[0]["branch_name"] = json!(FAILED.branch);
    let mut reader =
        MetadataOnlyReader::new(&PASSED, values.map(|v| serde_json::to_vec(&v).unwrap()));
    assert!(collect(&mut reader, &PASSED, &expected).is_err());
    reader.assert_exact_reads();
}

#[test]
fn historical_metadata_policy_matches_all_five_workflows_and_the_entire_unfiltered_catalog() {
    let config = crate::core::validation_runtime::proof_configuration(include_bytes!(
        "fixtures/legacy-activation.toml"
    ))
    .unwrap();
    assert!(config.validation_buildkite_preflights);
    let configured = config
        .ci_attested
        .expect("historical reviewed activation fixture is checked in");
    let frozen = accepted_mapping();
    let expected_workflows: BTreeSet<_> = frozen.iter().map(|row| row.workflow.as_str()).collect();
    let actual_workflows: BTreeSet<_> = configured
        .workflows
        .iter()
        .map(|workflow| workflow.path.as_str())
        .collect();
    assert_eq!(expected_workflows.len(), 5);
    assert_eq!(actual_workflows.len(), configured.workflows.len());
    assert_eq!(actual_workflows, expected_workflows);
    for workflow in &configured.workflows {
        let mut actual_keys = workflow.required_step_keys.clone();
        actual_keys.sort();
        let mut expected_keys: Vec<_> = frozen
            .iter()
            .filter(|row| row.workflow == workflow.path)
            .map(|row| row.step_key.clone())
            .collect();
        expected_keys.sort();
        assert_eq!(actual_keys, expected_keys, "{} native jobs", workflow.path);
        assert!(workflow.auxiliary_step_keys.is_empty());
        let trigger = workflow
            .pipeline_trigger
            .as_ref()
            .expect("explicit bootstrap trust");
        assert_eq!(trigger.contract, PipelineTriggerContract::BuildkiteGha01027);
        if workflow.path == WORKFLOW {
            assert_eq!(trigger, &policy());
        } else {
            assert!(trigger.concurrency_group.is_none());
        }
    }
    // No changed-path, release-only or applicability selection may trim the
    // accepted job mapping. Compare against the entire compiled source catalog.
    let expected_ids: BTreeSet<_> = frozen
        .iter()
        .flat_map(|row| row.checks.iter().map(String::as_str))
        .collect();
    let catalog_ids: BTreeSet<_> = source::CHECKS.iter().map(|check| check.id).collect();
    assert_eq!(expected_ids, catalog_ids);
    let mut expected = BTreeMap::new();
    for row in frozen {
        for check in row.checks {
            let source_check = source::CHECKS.iter().find(|item| item.id == check).unwrap();
            assert_eq!(
                row.workflow,
                format!(".github/workflows/{}", source_check.workflow)
            );
            assert_eq!(row.job, source_check.job);
            let mut catalog_matrix = BTreeMap::new();
            if let Some(value) = source_check.matrix {
                let axis = match source_check.job {
                    "musl-publication" | "test" => "target",
                    "state-commit-golden-vectors" => "os",
                    "test-integration" => "shard",
                    other => panic!("unreviewed matrix job {other}"),
                };
                catalog_matrix.insert(axis.to_string(), value.to_string());
                if source_check.job == "musl-publication" {
                    let execution = match value {
                        "x86_64-unknown-linux-musl" => "native",
                        "aarch64-unknown-linux-musl" => "QEMU user-mode",
                        other => panic!("unreviewed musl matrix {other}"),
                    };
                    catalog_matrix.insert("execution".into(), execution.into());
                }
            }
            assert_eq!(row.matrix, catalog_matrix, "{check} matrix");
            let key = CheckKey {
                provider: Provider::Buildkite,
                workflow: row.workflow.clone(),
                job: row.job.clone(),
                matrix: row.matrix.clone(),
                profile: format!("{REQUIRED_PROFILE}/{check}"),
            };
            assert!(expected.insert(key, row.step_key.clone()).is_none());
        }
    }
    expected.insert(
        CheckKey {
            provider: Provider::Buildkite,
            workflow: WORKFLOW.into(),
            job: "candidate-build".into(),
            matrix: BTreeMap::new(),
            profile: format!("{REQUIRED_PROFILE}/fresh-candidate-build"),
        },
        "gha-b803fcb7f17ed923-setup".into(),
    );
    let actual: BTreeMap<_, _> = configured
        .workflows
        .iter()
        .flat_map(|workflow| &workflow.checks)
        .map(|mapping| (mapping.check.clone(), mapping.step_key.clone()))
        .collect();
    assert_eq!(
        actual.len(),
        configured
            .workflows
            .iter()
            .map(|workflow| workflow.checks.len())
            .sum::<usize>(),
        "duplicate check mapping"
    );
    assert_eq!(actual, expected);
}

#[test]
fn repository_requires_strict_step_and_artifact_custody_instead_of_metadata_only() {
    let config = crate::core::validation_runtime::proof_configuration(include_bytes!(
        "../../../../../../.decapod/config.toml"
    ))
    .unwrap();
    assert!(config.validation_buildkite_preflights);
    assert!(config.ci_attested.is_none());
    assert_eq!(config.required_profile.unwrap().id, REQUIRED_PROFILE);
}
