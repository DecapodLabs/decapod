use super::*;
use serde_json::json;

const RUN: &str = "11111111-1111-1111-1111-111111111111";
const PIPELINE: &str = "22222222-2222-2222-2222-222222222222";
const JOBS: [&str; 3] = [
    "33333333-3333-3333-3333-333333333333",
    "44444444-4444-4444-4444-444444444444",
    "55555555-5555-5555-5555-555555555555",
];
const STEPS: [&str; 3] = [
    "66666666-6666-6666-6666-666666666666",
    "77777777-7777-7777-7777-777777777777",
    "88888888-8888-8888-8888-888888888888",
];
const KEYS: [&str; 3] = ["gha-format", "gha-test", "gha-concurrency-finish"];

struct MetadataReader {
    bodies: [Vec<u8>; 3],
    calls: Vec<PublicReadKind>,
}
impl PublicReader for MetadataReader {
    fn get(&mut self, request: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        let authorized = request.authorize_read()?;
        assert_eq!(authorized.method(), "GET");
        assert_eq!(authorized.max_https_redirects(), 0);
        let index = match authorized.kind() {
            PublicReadKind::BuildEnvelope => 0,
            PublicReadKind::FullSteps => 1,
            PublicReadKind::FullJobs => 2,
            _ => panic!("CI metadata must not request logs or artifacts"),
        };
        self.calls.push(authorized.kind());
        Ok(self.bodies[index].clone())
    }
}
fn build() -> PublicBuild {
    PublicBuild::new("org", "pipe", 7).unwrap()
}
fn expectation() -> InventoryExpectation {
    InventoryExpectation {
        repository: Repository::new("owner", "repo").unwrap(),
        pipeline_id: PIPELINE.into(),
        commit: "a".repeat(40),
        branch: "work".into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        event: "push".into(),
    }
}
fn required() -> Vec<String> {
    KEYS[..2].iter().map(|key| (*key).into()).collect()
}
fn auxiliary() -> Vec<String> {
    vec![KEYS[2].into()]
}
fn fixture() -> [Value; 3] {
    let build = build();
    let expected = expectation();
    let envelope = json!({
        "id": RUN, "number": 7, "path": build.base(),
        "commit_id": expected.commit, "branch_name": expected.branch,
        "github_workflow_path": expected.workflow_path,
        "github_webhook_event": expected.event, "state": "passed", "blocked_state": null,
        "account": {"slug": "org"},
        "project": {"id": PIPELINE, "slug": "pipe", "public": true,
            "provider": {"id": "github", "url": "https://github.com/owner/repo"}},
        "statistics": {"jobs_count": 3, "steps_count": 3}
    });
    let mut steps = Vec::new();
    let mut jobs = Vec::new();
    for index in 0..KEYS.len() {
        let job = JOBS[index];
        let step = STEPS[index];
        let key = KEYS[index];
        steps.push(json!({
            "uuid": step, "key": key, "type": "command", "command_script": "run-check",
            "state": "passed", "outcome": "passed", "uploaded_by_step_uuid": null,
            "statistics": {"retries_count": 0, "jobs_count": 1, "jobs_finished_count": 1,
                "step_children_count": 0, "latest_job_id": job}
        }));
        jobs.push(json!({
            "id": job, "step_uuid": step, "step_key": key, "command": "run-check",
            "matrix": null, "parallel_group_index": null, "parallel_group_total": null,
            "build_number": 7, "base_path": build.job_base(job).unwrap(), "type": "script",
            "retried_at": null, "retried_in_job_uuid": null, "retry_pending": false,
            "passed": true, "soft_failed": false, "stack_errored": false, "will_fail": false,
            "exit_status": 0, "state": "finished"
        }));
    }
    [
        envelope,
        json!(steps),
        json!({"records": jobs, "has_next_page": false, "has_previous_page": false}),
    ]
}
fn reader(values: [Value; 3]) -> MetadataReader {
    MetadataReader {
        bodies: values.map(|value| serde_json::to_vec(&value).unwrap()),
        calls: Vec::new(),
    }
}
fn collect(
    reader: &mut MetadataReader,
    expected: &InventoryExpectation,
    run: &str,
    required: &[String],
    auxiliary: &[String],
) -> Result<CiAttestedValidation> {
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(2), 1024 * 1024, &cancel).unwrap();
    collect_ci_attested_validation(reader, build(), expected, run, required, auxiliary, &budget)
}

#[test]
fn exact_independent_obligations_accept_only_complete_metadata_without_custody_reads() {
    let mut reader = reader(fixture());
    let result = collect(&mut reader, &expectation(), RUN, &required(), &auxiliary()).unwrap();
    assert_eq!(result.provider(), "buildkite");
    assert_eq!(result.repository().full_name(), "owner/repo");
    assert_eq!(result.repository_identity(), "github.com/owner/repo");
    assert_eq!(result.pipeline_id(), PIPELINE);
    assert_eq!(result.run_id(), RUN);
    assert_eq!(result.commit(), "a".repeat(40));
    assert_eq!(result.branch(), "work");
    assert_eq!(result.workflow_path(), ".github/workflows/ci.yml");
    assert_eq!(result.event(), "push");
    assert_eq!(result.jobs().len(), 3);
    assert_eq!(result.required_jobs().len(), 2);
    assert_eq!(result.auxiliary_jobs().len(), 1);
    for (key, job) in KEYS.into_iter().zip(JOBS) {
        assert_eq!(result.jobs()[key], job);
    }
    assert_eq!(result.required_jobs()[KEYS[0]], JOBS[0]);
    assert_eq!(result.auxiliary_jobs()[KEYS[2]], JOBS[2]);
    assert_eq!(
        reader.calls,
        vec![
            PublicReadKind::BuildEnvelope,
            PublicReadKind::FullSteps,
            PublicReadKind::FullJobs
        ]
    );
}

#[test]
fn metadata_digest_covers_exact_original_bodies_and_preserves_individual_digests() {
    let mut reader = reader(fixture());
    let mut transcript = b"decapod.public-buildkite.ci-metadata.v1\0".to_vec();
    for body in &reader.bodies {
        transcript.extend_from_slice(&(body.len() as u64).to_be_bytes());
        transcript.extend_from_slice(body);
    }
    let expected = hash(&transcript);
    let response_digests = reader.bodies.each_ref().map(|body| hash(body));
    let first = collect(&mut reader, &expectation(), RUN, &required(), &auxiliary()).unwrap();
    assert_eq!(first.metadata_digest(), expected);
    assert_eq!(first.response_digest(), expected);
    assert_eq!(first.response_digests(), &response_digests);

    // A presentation-only byte change does not change success but must change
    // the metadata evidence digest. It cannot be replaced by a synthetic hash.
    reader.bodies[0].push(b'\n');
    let second = collect(&mut reader, &expectation(), RUN, &required(), &auxiliary()).unwrap();
    assert_ne!(first.metadata_digest(), second.metadata_digest());
    assert_ne!(first.response_digests()[0], second.response_digests()[0]);
    assert_eq!(first.jobs(), second.jobs());
    assert_ne!(
        metadata_digest([b"a", b"bc", b""]),
        metadata_digest([b"ab", b"c", b""])
    );
}

#[test]
fn empty_duplicate_overlapping_or_invalid_controller_keys_fail_before_reads() {
    let cases = [
        (vec![], auxiliary()),
        (vec![KEYS[0].into(), KEYS[0].into()], auxiliary()),
        (required(), vec![KEYS[0].into()]),
        (required(), vec![KEYS[2].into(), KEYS[2].into()]),
        (vec!["../test".into()], auxiliary()),
        (vec!["".into()], auxiliary()),
    ];
    for (required, auxiliary) in cases {
        let mut reader = reader(fixture());
        assert!(collect(&mut reader, &expectation(), RUN, &required, &auxiliary).is_err());
        assert!(reader.calls.is_empty());
    }
}

#[test]
fn missing_unknown_duplicate_and_unkeyed_native_steps_cannot_shrink_obligations() {
    for case in 0..6 {
        let mut values = fixture();
        match case {
            0 => {
                // Even internally consistent reduced provider counts cannot
                // remove the independently required test job.
                values[1].as_array_mut().unwrap().remove(1);
                values[2]["records"].as_array_mut().unwrap().remove(1);
                values[0]["statistics"]["jobs_count"] = json!(2);
                values[0]["statistics"]["steps_count"] = json!(2);
            }
            1 => {
                values[1][1]["key"] = json!("unrelated-green-job");
                values[2]["records"][1]["step_key"] = json!("unrelated-green-job");
            }
            2 => {
                values[1][1]["key"] = json!(KEYS[0]);
                values[2]["records"][1]["step_key"] = json!(KEYS[0]);
            }
            3 => {
                values[1][2]["key"] = Value::Null;
                values[2]["records"][2]["step_key"] = Value::Null;
            }
            4 => {
                values[2]["records"][1]["id"] = json!(JOBS[0]);
                values[2]["records"][1]["base_path"] = json!(build().job_base(JOBS[0]).unwrap());
                values[1][1]["statistics"]["latest_job_id"] = json!(JOBS[0]);
            }
            5 => {
                values[2]["records"][1]["step_uuid"] = json!(STEPS[0]);
            }
            _ => unreachable!(),
        }
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err(),
            "case {case}"
        );
    }
    // Extras are accepted only through the independently supplied auxiliary set.
    assert!(
        collect(
            &mut reader(fixture()),
            &expectation(),
            RUN,
            &required(),
            &[]
        )
        .is_err()
    );
    let mut extra_required = required();
    extra_required.push("required-but-absent".into());
    assert!(
        collect(
            &mut reader(fixture()),
            &expectation(),
            RUN,
            &extra_required,
            &auxiliary()
        )
        .is_err()
    );
}

#[test]
fn failed_skipped_running_cancelled_soft_failed_and_nonzero_jobs_fail_closed() {
    for state in [
        "failed", "skipped", "running", "canceled", "broken", "unknown",
    ] {
        let mut values = fixture();
        values[2]["records"][1]["state"] = json!(state);
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err(),
            "state {state}"
        );
    }
    for (field, value) in [
        ("passed", json!(false)),
        ("soft_failed", json!(true)),
        ("stack_errored", json!(true)),
        ("will_fail", json!(true)),
        ("exit_status", json!(1)),
        ("exit_status", Value::Null),
    ] {
        let mut values = fixture();
        values[2]["records"][1][field] = value;
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err(),
            "field {field}"
        );
    }
    // Explicit auxiliary jobs are also mandatory successes, not exemptions.
    let mut values = fixture();
    values[2]["records"][2]["passed"] = json!(false);
    assert!(
        collect(
            &mut reader(values),
            &expectation(),
            RUN,
            &required(),
            &auxiliary()
        )
        .is_err()
    );
}

#[test]
fn failed_blocked_and_nonterminal_builds_do_not_attest_success() {
    for state in [
        "failed", "skipped", "running", "canceled", "blocked", "unknown",
    ] {
        let mut values = fixture();
        values[0]["state"] = json!(state);
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err()
        );
    }
    let mut values = fixture();
    values[0]["blocked_state"] = json!("blocked");
    assert!(
        collect(
            &mut reader(values),
            &expectation(),
            RUN,
            &required(),
            &auxiliary()
        )
        .is_err()
    );
}

#[test]
fn retried_incomplete_and_malformed_metadata_fail_closed() {
    for case in 0..11 {
        let mut values = fixture();
        match case {
            0 => values[1][1]["statistics"]["retries_count"] = json!(1),
            1 => values[2]["records"][1]["retried_at"] = json!("2026-01-01T00:00:00Z"),
            2 => values[2]["records"][1]["retried_in_job_uuid"] = json!(JOBS[0]),
            3 => values[2]["records"][1]["retry_pending"] = json!(true),
            4 => values[2]["has_next_page"] = json!(true),
            5 => values[2]["has_previous_page"] = json!(true),
            6 => {
                values[2]["records"].as_array_mut().unwrap().pop();
            }
            7 => {
                values[1].as_array_mut().unwrap().pop();
            }
            8 => {
                values[2]["records"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("soft_failed");
            }
            9 => values[2]["records"][1]["exit_status"] = json!("0"),
            10 => values[1][1]["statistics"]["step_children_count"] = json!(1),
            _ => unreachable!(),
        }
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err(),
            "case {case}"
        );
    }
}

#[test]
fn wrong_source_run_workflow_event_pipeline_repository_and_job_identity_fail() {
    for case in 0..9 {
        let mut values = fixture();
        match case {
            0 => values[0]["commit_id"] = json!("b".repeat(40)),
            1 => values[0]["branch_name"] = json!("other-work"),
            2 => values[0]["id"] = json!(JOBS[0]),
            3 => values[0]["github_workflow_path"] = json!(".github/workflows/other.yml"),
            4 => values[0]["github_webhook_event"] = json!("workflow_dispatch"),
            5 => values[0]["project"]["id"] = json!(JOBS[0]),
            6 => values[0]["project"]["provider"]["url"] = json!("https://github.com/other/repo"),
            7 => values[2]["records"][1]["build_number"] = json!(8),
            8 => values[2]["records"][1]["base_path"] = json!(build().job_base(JOBS[0]).unwrap()),
            _ => unreachable!(),
        }
        assert!(
            collect(
                &mut reader(values),
                &expectation(),
                RUN,
                &required(),
                &auxiliary()
            )
            .is_err(),
            "case {case}"
        );
    }
}

#[test]
fn verified_inventory_cannot_be_rebound_to_another_expected_head_or_run() {
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(2), 1024 * 1024, &cancel).unwrap();
    let inventory =
        collect_inventory(&mut reader(fixture()), build(), &expectation(), &budget).unwrap();
    let mut other_head = expectation();
    other_head.commit = "b".repeat(40);
    assert!(
        verify_ci_attested_validation(&inventory, &other_head, RUN, &required(), &auxiliary())
            .is_err()
    );
    assert!(
        verify_ci_attested_validation(
            &inventory,
            &expectation(),
            JOBS[0],
            &required(),
            &auxiliary()
        )
        .is_err()
    );
}
