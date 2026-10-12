use super::*;
use serde_json::json;
const BUILD: &str = "11111111-1111-1111-1111-111111111111";
const PIPE: &str = "22222222-2222-2222-2222-222222222222";
const IMPORTER: &str = "33333333-3333-3333-3333-333333333333";
const JOB: &str = "44444444-4444-4444-4444-444444444444";
const ISTEP: &str = "55555555-5555-5555-5555-555555555555";
const STEP: &str = "66666666-6666-6666-6666-666666666666";
const AID: &str = "77777777-7777-7777-7777-777777777777";
const PID: &str = "88888888-8888-8888-8888-888888888888";
const EID: &str = "99999999-9999-9999-9999-999999999999";
struct Mock {
    responses: BTreeMap<String, Vec<u8>>,
    calls: usize,
}
impl PublicReader for Mock {
    fn get(&mut self, r: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        self.calls += 1;
        self.responses
            .get(&r.path)
            .cloned()
            .ok_or_else(|| invalid("unexpected route"))
    }
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}
fn build() -> PublicBuild {
    PublicBuild::new("org", "pipe", 7).unwrap()
}
fn expected() -> InventoryExpectation {
    InventoryExpectation {
        repository: Repository::new("owner", "repo").unwrap(),
        pipeline_id: PIPE.into(),
        commit: "a".repeat(40),
        branch: "work".into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        event: "push".into(),
    }
}
fn recipe_expected() -> RecipeExpectation {
    RecipeExpectation {
        workflow_path: ".github/workflows/ci.yml".into(),
        workflow_digest: hash(b"source"),
        logical_job: "test".into(),
        compiler_version: "0.102.2".into(),
        runtime_digest: hash(b"runtime"),
        matrix: Default::default(),
        job_index: 0,
        job_total: 1,
    }
}
fn fixture(result: &str) -> Mock {
    let b = build();
    let e = expected();
    let envelope = json!({"id":BUILD,"number":7,"path":b.base(),"commit_id":e.commit,"branch_name":"work","github_workflow_path":e.workflow_path,"github_webhook_event":"push","blocked_state":null,"state":"passed","account":{"slug":"org"},"project":{"id":PIPE,"slug":"pipe","public":true,"provider":{"id":"github","url":"https://github.com/owner/repo"}},"statistics":{"jobs_count":2,"steps_count":2}});
    let mut steps = vec![];
    let mut jobs = vec![];
    for (id, step, key, producer) in [
        (IMPORTER, ISTEP, None, None),
        (JOB, STEP, Some("gha-test"), Some(ISTEP)),
    ] {
        steps.push(json!({"uuid":step,"key":key,"type":"command","command_script":null,"uploaded_by_step_uuid":producer,"statistics":{"retries_count":0,"jobs_count":1,"step_children_count":0,"latest_job_id":id}}));
        jobs.push(json!({"id":id,"step_uuid":step,"step_key":key,"command":null,"matrix":null,"parallel_group_index":null,"parallel_group_total":null,"build_number":7,"base_path":b.job_base(id).unwrap(),"type":"script","retried_at":null,"retried_in_job_uuid":null,"retry_pending":false,"passed":true,"soft_failed":false,"stack_errored":false,"will_fail":false,"exit_status":0,"state":"finished"}));
    }
    let event = b"event bytes";
    let plan = bytes(
        &json!({"schema":"https://buildkite.com/schemas/buildkite-gha/job-plan-v2.schema.json","workflow":{"path":"./.github/workflows/ci.yml","digest":hash(b"source"),"logical_job_id":"test"},"compiler":{"version":"0.102.2","distribution_digest":hash(b"runtime")},"runtime":{"distribution_digest":hash(b"runtime")},"target":{"step_key":"gha-test"},"strategy":{"job-index":0,"job-total":1},"event":{"provider":"github","name":"push","sha":"a".repeat(40),"repository":"owner/repo","payload_digest":hash(event),"payload_artifact":true},"program":{"version":2,"job":{"steps":[]}}}),
    );
    let pd = hash(&plan);
    let manifest = bytes(
        &json!({"schema":"buildkite-gha/result-manifest/v2","plan_digest":pd,"producer":{"build_id":BUILD,"job_id":JOB,"step_key":"gha-test"},"result":result,"outputs":[],"artifacts":[]}),
    );
    let mut responses = BTreeMap::from([
        (format!("{}.json", b.base()), bytes(&envelope)),
        (format!("{}/data/steps", b.base()), bytes(&json!(steps))),
        (
            format!(
                "{}/data/jobs?include_retried_jobs=true&paginate=false",
                b.base()
            ),
            bytes(&json!({"records":jobs,"has_next_page":false,"has_previous_page":false})),
        ),
    ]);
    let mut listings: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (job, aid, path, data) in [
        (
            JOB,
            AID,
            format!("buildkite-gha/v1/results/gha-test/{}.json", &pd[7..]),
            manifest,
        ),
        (
            IMPORTER,
            PID,
            format!(".buildkite-gha/plans/{}.json", &pd[7..]),
            plan,
        ),
        (
            IMPORTER,
            EID,
            format!(".buildkite-gha/events/{}.json", &hash(event)[7..]),
            event.to_vec(),
        ),
    ] {
        let url = format!("{}/artifacts/{aid}", b.job_base(job).unwrap());
        listings.entry(job.into()).or_default().push(json!({"id":aid,"state":"finished","path":path,"url":url,"self_hosted":false,"sha256sum":&hash(&data)[7..]}));
        responses.insert(url, data);
    }
    for (job, rows) in listings {
        responses.insert(
            format!("{}/artifacts", b.job_base(&job).unwrap()),
            bytes(&json!(rows)),
        );
    }
    Mock {
        responses,
        calls: 0,
    }
}
fn mutate(mock: &mut Mock, path: String, change: impl FnOnce(&mut Value)) {
    let mut v = json(&mock.responses[&path]).unwrap();
    change(&mut v);
    mock.responses.insert(path, bytes(&v));
}
fn budget(c: &Cancellation) -> Budget<'_> {
    Budget::new(Duration::from_secs(2), 1024 * 1024, c).unwrap()
}
#[test]
fn complete_native_exit_zero_does_not_override_logical_skip() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("skipped");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    assert!(i.all_native_scripts_passed());
    let r = collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b).unwrap();
    assert_eq!(r.logical_result(), "skipped");
}
#[test]
fn actual_failure_manifest_stays_failure() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("failure");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    assert_eq!(
        collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b)
            .unwrap()
            .logical_result(),
        "failure"
    );
}
#[test]
fn incomplete_pagination_is_not_complete() {
    let c = Cancellation::default();
    let mut m = fixture("success");
    mutate(
        &mut m,
        format!(
            "{}/data/jobs?include_retried_jobs=true&paginate=false",
            build().base()
        ),
        |v| v["has_next_page"] = true.into(),
    );
    assert!(collect_inventory(&mut m, build(), &expected(), &budget(&c)).is_err());
}
#[test]
fn missing_fields_have_no_rest_defaults() {
    let c = Cancellation::default();
    let mut m = fixture("success");
    mutate(
        &mut m,
        format!(
            "{}/data/jobs?include_retried_jobs=true&paginate=false",
            build().base()
        ),
        |v| {
            v["records"][0]
                .as_object_mut()
                .unwrap()
                .remove("soft_failed");
        },
    );
    assert!(collect_inventory(&mut m, build(), &expected(), &budget(&c)).is_err());
}
#[test]
fn retries_and_duplicate_jobs_reject() {
    for duplicate in [true, false] {
        let c = Cancellation::default();
        let mut m = fixture("success");
        mutate(
            &mut m,
            format!(
                "{}/data/jobs?include_retried_jobs=true&paginate=false",
                build().base()
            ),
            |v| {
                if duplicate {
                    v["records"][1] = v["records"][0].clone()
                } else {
                    v["records"][0]["retry_pending"] = true.into()
                }
            },
        );
        assert!(collect_inventory(&mut m, build(), &expected(), &budget(&c)).is_err());
    }
}
#[test]
fn filtered_steps_cannot_close_inventory() {
    let c = Cancellation::default();
    let mut m = fixture("success");
    mutate(&mut m, format!("{}/data/steps", build().base()), |v| {
        v.as_array_mut().unwrap().pop();
    });
    assert!(collect_inventory(&mut m, build(), &expected(), &budget(&c)).is_err());
}
#[test]
fn artifact_destination_cannot_redirect_to_arbitrary_url() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    mutate(
        &mut m,
        format!("{}/artifacts", build().job_base(JOB).unwrap()),
        |v| v[0]["url"] = "https://example.invalid/collect".into(),
    );
    assert!(collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b).is_err());
}
#[test]
fn modified_artifact_bytes_fail_provider_hash() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    m.responses.insert(
        format!("{}/artifacts/{AID}", build().job_base(JOB).unwrap()),
        b"{}".to_vec(),
    );
    assert!(collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b).is_err());
}
#[test]
fn independent_workflow_runtime_matrix_must_match() {
    for n in 0..3 {
        let c = Cancellation::default();
        let b = budget(&c);
        let mut m = fixture("success");
        let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
        let mut e = recipe_expected();
        match n {
            0 => e.workflow_digest = hash(b"different"),
            1 => e.runtime_digest = hash(b"different"),
            _ => {
                e.matrix.insert("shard".into(), "unexpected".into());
            }
        }
        assert!(collect_recipe(&mut m, &i, JOB, &e, &b).is_err());
    }
}
#[test]
fn changed_inventory_rejects_final_readback() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    mutate(&mut m, format!("{}.json", build().base()), |v| {
        v["state"] = "failed".into()
    });
    assert!(recheck_inventory(&mut m, &i, &expected(), &b).is_err());
}
#[test]
fn precancellation_never_calls_transport() {
    let c = Cancellation::default();
    c.cancel();
    let mut m = fixture("success");
    assert!(collect_inventory(&mut m, build(), &expected(), &budget(&c)).is_err());
    assert_eq!(m.calls, 0);
}
#[test]
fn malicious_locators_rejected() {
    assert!(PublicBuild::new("org/../else", "p", 1).is_err());
    assert!(PublicBuild::new("org", "p", 0).is_err());
    assert!(build().job_base("../anything").is_err());
}
#[test]
fn presentation_metadata_does_not_invalidate_execution_identity() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    mutate(
        &mut m,
        format!(
            "{}/data/jobs?include_retried_jobs=true&paginate=false",
            build().base()
        ),
        |v| v["records"][0]["hosted_concurrency"] = json!({"running_jobs_count":17}),
    );
    recheck_inventory(&mut m, &i, &expected(), &b).unwrap();
}
#[test]
fn log_is_measured_for_exact_inventory_job_without_step_inference() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    m.responses.insert(
        format!("{}/download.txt", build().job_base(JOB).unwrap()),
        b"--- potentially skipped label\n".to_vec(),
    );
    let log = collect_log(&mut m, &i, JOB, &b).unwrap();
    assert_eq!(log.digest(), hash(log.bytes()));
    assert_eq!(log.job_id(), JOB);
    assert!(collect_log(&mut m, &i, AID, &b).is_err());
}
#[test]
fn sealed_authorizer_rejects_post_like_queries_and_wrong_redirects() {
    for (path, redirect) in [
        ("/org/pipe/builds/7/data/jobs?state=passed", false),
        ("/org/pipe/builds/7.json", true),
        ("//evil.invalid/org/pipe/builds/7.json", false),
        ("/org/pipe/builds/7/cancel", false),
    ] {
        assert!(
            PublicRequest {
                path: path.into(),
                artifact_redirect: redirect
            }
            .authorize_read()
            .is_err()
        );
    }
    let seal = PublicRequest {
        path: format!("{}/artifacts/{AID}", build().job_base(JOB).unwrap()),
        artifact_redirect: true,
    }
    .authorize_read()
    .unwrap();
    assert_eq!(seal.method(), "GET");
    assert_eq!(seal.kind(), PublicReadKind::ArtifactContent);
    assert_eq!(seal.max_https_redirects(), 3);
}
#[test]
fn redirects_are_validated_before_following_and_bound_to_exact_producer() {
    let request = PublicRequest {
        path: format!("{}/artifacts/{AID}", build().job_base(JOB).unwrap()),
        artifact_redirect: true,
    };
    let valid = format!(
        "https://s3.amazonaws.com/buildkiteartifacts.com/{BUILD}/{PIPE}/{ISTEP}/{JOB}/.buildkite-gha/plans/file.json?opaque=signature"
    );
    validate_storage_redirect(&request, &valid).unwrap();
    for bad in [
        valid.replace("s3.amazonaws.com", "evil.invalid"),
        valid.replace("https://", "http://"),
        valid.replace("s3.amazonaws.com/", "s3.amazonaws.com:444/"),
        valid.replace("s3.amazonaws.com/", "name@s3.amazonaws.com/"),
        valid.replace("buildkiteartifacts.com/", "another-bucket/"),
        valid.replace(JOB, IMPORTER),
        valid.replace("file.json", "../file.json"),
        valid.replace("file.json", "%2e%2e/file.json"),
        format!("{valid}#fragment"),
    ] {
        assert!(validate_storage_redirect(&request, &bad).is_err());
    }
}
#[test]
fn http_parser_rejects_ambiguous_locations_and_keeps_raw_body() {
    assert!(http_response(b"HTTP/1.1 302 Found\r\nLocation: a\r\nlocation: b\r\n\r\n").is_err());
    let (status, location, body) =
        http_response(b"HTTP/2 200\r\nContent-Type: application/octet-stream\r\n\r\n\x00binary\n")
            .unwrap();
    assert_eq!(status, 200);
    assert!(location.is_none());
    assert_eq!(body, b"\x00binary\n");
}
#[test]
fn imported_source_archive_is_bound_to_manifest_site_digest_and_real_job() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let z = super::super::artifact_archive::tests::zip(
        "fmt/preparation/preparation.json",
        b"{}",
        0x8000,
    );
    let result_path = format!("{}/artifacts/{AID}", build().job_base(JOB).unwrap());
    mutate(
        &mut m,
        result_path.clone(),
        |v| v["artifacts"] = json!([{"name":"source-proof","id":"opaque","path":"buildkite-gha/v1/artifacts/content.zip","digest":hash(&z),"size":z.len(),"file_count":1}]),
    );
    let result_sha = hash(&m.responses[&result_path]);
    let zip_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let zip_url = format!("{}/artifacts/{zip_id}", build().job_base(JOB).unwrap());
    mutate(
        &mut m,
        format!("{}/artifacts", build().job_base(JOB).unwrap()),
        |v| {
            v[0]["sha256sum"] = result_sha[7..].into();
            v.as_array_mut().unwrap().push(json!({"id":zip_id,"state":"finished","path":"buildkite-gha/v1/artifacts/content.zip","url":zip_url,"self_hosted":false,"sha256sum":&hash(&z)[7..]}));
        },
    );
    m.responses.insert(zip_url, z);
    let inventory = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    let recipe = collect_recipe(&mut m, &inventory, JOB, &recipe_expected(), &b).unwrap();
    let files = collect_named_archive(&mut m, &inventory, &recipe, "source-proof", &b).unwrap();
    assert_eq!(
        files.file("fmt/preparation/preparation.json").unwrap(),
        b"{}"
    );
}
#[test]
fn native_exit_zero_importer_skip_never_yields_source_execution_bundle() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("skipped");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    let r = collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b).unwrap();
    assert!(collect_named_archive(&mut m, &i, &r, "source-proof", &b).is_err());
}
#[test]
fn located_recipe_reuses_authenticated_plan_and_requires_independent_match() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    assert!(locate_recipe(&mut m, &i, IMPORTER, &b).unwrap().is_none());
    let l = locate_recipe(&mut m, &i, JOB, &b).unwrap().unwrap();
    assert_eq!(l.producer_job_id(), JOB);
    assert_eq!(l.logical_job(), "test");
    assert_eq!((l.job_index(), l.job_total()), (0, 1));
    assert!(l.matrix().is_empty());
    let before = m.calls;
    let r = collect_located_recipe(&mut m, &i, l, &recipe_expected(), &b).unwrap();
    assert_eq!(m.calls - before, 2);
    assert_eq!(r.logical_result(), "success");
    let l = locate_recipe(&mut m, &i, JOB, &b).unwrap().unwrap();
    let mut wrong = recipe_expected();
    wrong.logical_job = "unrelated".into();
    assert!(collect_located_recipe(&mut m, &i, l, &wrong, &b).is_err());
}
#[test]
fn missing_importer_result_is_only_a_locator_absence() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut m = fixture("success");
    let i = collect_inventory(&mut m, build(), &expected(), &b).unwrap();
    m.responses.insert(
        format!("{}/artifacts", build().job_base(JOB).unwrap()),
        b"[]".to_vec(),
    );
    assert!(locate_recipe(&mut m, &i, JOB, &b).unwrap().is_none());
    assert!(collect_recipe(&mut m, &i, JOB, &recipe_expected(), &b).is_err());
}

/// Synthetic provider test double only. Exercise the real inventory/recipe
/// verifier so source tests cannot mint an imported recipe from receipt JSON.
pub(crate) fn synthetic_source_recipe() -> ImportedRecipe {
    let mut reader = fixture("success");
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(3), 1024 * 1024, &cancel).unwrap();
    let inventory = collect_inventory(&mut reader, build(), &expected(), &budget).unwrap();
    collect_recipe(&mut reader, &inventory, JOB, &recipe_expected(), &budget).unwrap()
}

#[cfg(unix)]
fn timeout_curl_fixture(body: &str) -> (tempfile::TempDir, PathBuf) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("curl-fixture");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)
        .unwrap();
    writeln!(file, "#!/bin/sh\ncounter=\"${{0}}.count\"\nn=0\nif test -f \"$counter\"; then read -r n <\"$counter\"; fi\nn=$((n + 1))\nprintf '%s\\n' \"$n\" >\"$counter\"\n{body}").unwrap();
    (temp, path)
}

#[cfg(unix)]
fn timeout_request() -> PublicRequest {
    PublicRequest {
        path: "/org/pipe/builds/7.json".into(),
        artifact_redirect: false,
    }
}

#[cfg(unix)]
fn timeout_attempts(path: &Path) -> usize {
    std::fs::read_to_string(format!("{}.count", path.display()))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn timeout_read_retries_only_current_get_and_discards_partial_responses() {
    let (_temp, path) = timeout_curl_fixture(
        "if test \"$n\" -lt 3; then printf 'incomplete response'; exit 28; fi\nprintf 'HTTP/2 200\\r\\n\\r\\ncomplete'",
    );
    let mut reader = CurlPublicReader::new(path.clone(), |_, _| Ok(())).unwrap();
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(10), 4096, &cancel).unwrap();
    assert_eq!(
        reader.get(&timeout_request(), &budget).unwrap(),
        b"complete"
    );
    assert_eq!(timeout_attempts(&path), 3);
}

#[cfg(unix)]
#[test]
fn timeout_read_exhaustion_is_nonzero_and_bounded() {
    let (_temp, path) = timeout_curl_fixture("exit 28");
    let mut reader = CurlPublicReader::new(path.clone(), |_, _| Ok(())).unwrap();
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(10), 4096, &cancel).unwrap();
    let error = reader.get(&timeout_request(), &budget).unwrap_err();
    assert!(error.to_string().contains("bounded_timeout_attempts=3"));
    assert_eq!(timeout_attempts(&path), 3);
}

#[cfg(unix)]
#[test]
fn timeout_read_never_retries_http_authorization_tls_or_malformed_failures() {
    for body in [
        "printf 'HTTP/2 401\\r\\n\\r\\n'; exit 22",
        "printf 'HTTP/2 403\\r\\n\\r\\n'; exit 22",
        "printf 'HTTP/2 429\\r\\nRetry-After: 60\\r\\n\\r\\n'; exit 22",
        "exit 60",
        "printf 'invalid provider response'",
    ] {
        let (_temp, path) = timeout_curl_fixture(body);
        let mut reader = CurlPublicReader::new(path.clone(), |_, _| Ok(())).unwrap();
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_secs(10), 4096, &cancel).unwrap();
        assert!(reader.get(&timeout_request(), &budget).is_err());
        assert_eq!(timeout_attempts(&path), 1, "{body}");
    }
}

#[cfg(unix)]
#[test]
fn timeout_retry_shares_deadline_and_respects_denied_authorization() {
    let (_temp, path) = timeout_curl_fixture("exit 28");
    let mut reader = CurlPublicReader::new(path.clone(), |_, _| Ok(())).unwrap();
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_millis(100), 4096, &cancel).unwrap();
    assert_eq!(
        reader.get(&timeout_request(), &budget).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(timeout_attempts(&path), 1);
    let (_temp, path) = timeout_curl_fixture("exit 28");
    let mut reader = CurlPublicReader::new(path.clone(), |_, _| {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
    })
    .unwrap();
    let budget = Budget::new(Duration::from_secs(10), 4096, &cancel).unwrap();
    assert_eq!(
        reader.get(&timeout_request(), &budget).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(!Path::new(&format!("{}.count", path.display())).exists());
}
