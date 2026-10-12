use super::*;
use serde_json::json;
const ORIGIN: &str = "buildkite:11111111-1111-1111-1111-111111111111/jobs/22222222-2222-2222-2222-222222222222/source-proof";
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}
fn expected() -> SourceExpectation {
    SourceExpectation {
        workflow: "ci.yml".into(),
        job: "fmt".into(),
        check: "fmt".into(),
        matrix: None,
        platform: "linux-x86_64".into(),
        base: "a".repeat(40),
        head: "b".repeat(40),
        snapshot: "c".repeat(40),
        execution_revision: "b".repeat(40),
        tree: "d".repeat(40),
        provider: "buildkite".into(),
        run_id: "11111111-1111-1111-1111-111111111111".into(),
        run_attempt: "0".into(),
        job_id: "22222222-2222-2222-2222-222222222222".into(),
        step_key: "gha-fmt".into(),
        profile_digest: hash(b"independent profile"),
        invocations: bytes(
            &json!([{"program":"cargo","args":["fmt","--all","--","--check"],"cwd":".","env":{}}]),
        ),
        artifact_source_paths: Default::default(),
        execution_mode: "source_checkout".into(),
    }
}
fn log(path: &str, data: &[u8]) -> Value {
    json!({"path":path,"bytes":data.len(),"stored_bytes":data.len(),"truncated":false,"sha256":hash(data),"stored_sha256":hash(data)})
}
fn fixture() -> BTreeMap<String, Vec<u8>> {
    let e = expected();
    let mut files = BTreeMap::new();
    let context = bytes(&json!({"RUST_BACKTRACE":"1"}));
    let exe = hash(b"actual cargo file");
    let toolchain = bytes(
        &json!([{"program":"cargo","executable_sha256":exe,"version_args":["--version"],"exit_code":0,"signal":null,"success":true,"stdout_sha256":hash(b"cargo fixture-version\n"),"stdout_bytes":22,"stderr_sha256":hash(b""),"stderr_bytes":0}]),
    );
    let mut prep = json!({"schema":"decapod.source-ci.preparation.v1","authenticated":false,"complete_pr_profile":false,"preparation_complete":true,"workflow":e.workflow,"job":e.job,"check":e.check,"matrix":null,"platform":e.platform,"base":e.base,"head":e.head,"snapshot":e.snapshot,"execution_revision":e.execution_revision,"tree":e.tree,"provider":e.provider,"run_id":e.run_id,"run_attempt":e.run_attempt,"job_id":e.job_id,"step_key":e.step_key,"profile_digest":e.profile_digest,"invocation_digest":hash(&e.invocations),"toolchain_digest":hash(&toolchain),"context_digest":hash(&context),"executor_digest":hash(b"executor")});
    // Count from the actual fixture stream, never a duplicated invented length.
    let mut tc = value(&toolchain).unwrap();
    tc[0]["stdout_bytes"] = (b"cargo fixture-version\n".len() as u64).into();
    let toolchain = bytes(&tc);
    prep["toolchain_digest"] = hash(&toolchain).into();
    let mut prepbytes = bytes(&prep);
    prepbytes.push(b'\n');
    for phase in ["preparation", "execution"] {
        let root = format!("fmt/{phase}/");
        for (name, data) in [
            ("preparation.json", prepbytes.clone()),
            ("context.json", context.clone()),
            ("invocations.json", e.invocations.clone()),
            ("toolchain.json", toolchain.clone()),
        ] {
            files.insert(format!("{root}{name}"), data);
        }
        for stem in if phase == "execution" {
            vec!["", "after-"]
        } else {
            vec![""]
        } {
            let stdout = format!("{stem}tool-0.stdout.log");
            let stderr = format!("{stem}tool-0.stderr.log");
            let logs = bytes(
                &json!([{"program":"cargo","executable":"/tools/cargo","stdout":log(&stdout,b"cargo fixture-version\n"),"stderr":log(&stderr,b"")}]),
            );
            files.insert(format!("{root}{stem}tool-logs.json"), logs);
            files.insert(
                format!("{root}{stdout}"),
                b"cargo fixture-version\n".to_vec(),
            );
            files.insert(format!("{root}{stderr}"), vec![]);
        }
    }
    files.insert(
        "fmt/execution/frozen-preparation.json".into(),
        prepbytes.clone(),
    );
    files.insert("fmt/execution/after-toolchain.json".into(), toolchain);
    let record = json!({"previous_record_sha256":hash(&prepbytes),"invocation":value(&e.invocations).unwrap()[0],"executable":"/tools/cargo","executable_sha256":exe,"exit_code":0,"signal":null,"success":true,"elapsed_ms":1,"stdout":log("command-0.stdout.log",b""),"stderr":log("command-0.stderr.log",b"")});
    let mut execution = prep;
    let obj = execution.as_object_mut().unwrap();
    for key in ["preparation_complete", "tree", "executor_digest"] {
        obj.remove(key);
    }
    for (k, v) in [
        ("schema", json!("decapod.source-ci.execution.v2")),
        (
            "final_execution_revision",
            e.execution_revision.clone().into(),
        ),
        ("execution_complete", true.into()),
        ("logs_complete", true.into()),
        ("status", "passed".into()),
        ("candidate_tree", e.tree.clone().into()),
        ("snapshot_tree", e.tree.into()),
        ("execution_mode", e.execution_mode.into()),
        ("preparation_digest", hash(&prepbytes).into()),
        ("terminal_record_sha256", hash(&bytes(&record)).into()),
        ("results", json!([record])),
        ("artifacts", json!([])),
        ("error", Value::Null),
        ("elapsed_ms", 1.into()),
    ] {
        obj.insert(k.into(), v);
    }
    files.insert("fmt/execution/execution.json".into(), bytes(&execution));
    files.insert("fmt/execution/command-0.stdout.log".into(), vec![]);
    files.insert("fmt/execution/command-0.stderr.log".into(), vec![]);
    files
}
fn policy(p: MeasuredPreparation<'_>) -> Result<()> {
    ensure(
        p.toolchain.as_array().is_some_and(|r| r.len() == 1)
            && p.toolchain[0]["program"] == "cargo"
            && p.context == &json!({"RUST_BACKTRACE":"1"}),
        "fixture policy mismatch",
    )
}
fn mutate(files: &mut BTreeMap<String, Vec<u8>>, path: &str, f: impl FnOnce(&mut Value)) {
    let mut v = value(&files[path]).unwrap();
    f(&mut v);
    files.insert(path.into(), bytes(&v));
}
#[test]
fn actual_command_logs_chain_and_independent_recipe_required() {
    let files = ArtifactFiles::fixture(ORIGIN, fixture());
    let p = verify_preparation(&files, &expected(), policy).unwrap();
    let x = verify_execution(&files, &p).unwrap();
    assert_eq!(x.invocation_digest(), hash(&expected().invocations));
    assert!(x.artifacts().is_empty());
    assert!(matches!(
        x.observation(),
        readiness_evidence::Observation::Executed {
            exit_code: Some(0),
            ..
        }
    ));
}
#[test]
fn trusted_preparation_policy_is_mandatory_and_may_deny() {
    let files = ArtifactFiles::fixture(ORIGIN, fixture());
    assert!(
        verify_preparation(&files, &expected(), |_| Err(invalid(
            "nested toolchain unsupported"
        )))
        .is_err()
    );
}
#[test]
fn independent_recipe_and_provider_identity_cannot_be_observed_defaults() {
    for changed in [true, false] {
        let files = ArtifactFiles::fixture(ORIGIN, fixture());
        let mut e = expected();
        if changed {
            e.job_id = "33333333-3333-3333-3333-333333333333".into();
        } else {
            e.invocations = bytes(&json!([{"program":"true","args":[],"cwd":".","env":{}}]));
        }
        assert!(verify_preparation(&files, &e, policy).is_err());
    }
}
#[test]
fn skipped_missing_nonzero_signal_and_broken_chain_never_green() {
    for n in 0..6 {
        let mut f = fixture();
        mutate(&mut f, "fmt/execution/execution.json", |v| match n {
            0 => v["status"] = "not_applicable".into(),
            1 => v["results"] = json!([]),
            2 => v["results"][0]["exit_code"] = 1.into(),
            3 => v["results"][0]["signal"] = 9.into(),
            4 => v["results"][0]["previous_record_sha256"] = hash(b"unrelated").into(),
            _ => v["terminal_record_sha256"] = hash(b"unrelated").into(),
        });
        let files = ArtifactFiles::fixture(ORIGIN, f);
        let p = verify_preparation(&files, &expected(), policy).unwrap();
        assert!(verify_execution(&files, &p).is_err());
    }
}
#[test]
fn mutated_raw_output_toolchain_context_and_producer_reject() {
    for name in [
        "fmt/execution/command-0.stdout.log",
        "fmt/execution/after-toolchain.json",
        "fmt/execution/context.json",
    ] {
        let mut f = fixture();
        f.insert(name.into(), b"tampered".to_vec());
        let files = ArtifactFiles::fixture(ORIGIN, f);
        let p = verify_preparation(&files, &expected(), policy).unwrap();
        assert!(verify_execution(&files, &p).is_err());
    }
    let a = ArtifactFiles::fixture(ORIGIN, fixture());
    let p = verify_preparation(&a, &expected(), policy).unwrap();
    let b = ArtifactFiles::fixture("two", fixture());
    assert!(verify_execution(&b, &p).is_err());
}
#[test]
fn duplicate_json_fields_reject_at_every_depth() {
    for json in [
        b"{\"status\":\"failed\",\"status\":\"passed\"}".as_slice(),
        b"{\"nested\":{\"exit\":1,\"exit\":0}}",
    ] {
        assert!(value(json).is_err());
    }
}
#[test]
fn raw_chain_preserves_key_order_and_quoted_braces() {
    let raw = br#"{"results":[{"z":"quoted } \\\"","a":[1]}, {"b":2}]}"#;
    let rows = raw_results(raw).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with(br#"{"z""#));
}
// Offline, generated parser vectors. No fixture is a captured build record or
// authenticated provider result; ArtifactFiles::fixture exists only in tests.
const SYNTHETIC_ORIGIN: &str = "github:synthetic/fixture/runs/101/attempts/1/artifacts/103";
fn synthetic_expected(version: u8) -> SourceExpectation {
    let mut e = expected();
    e.provider = "github".into();
    e.run_id = "101".into();
    e.run_attempt = "1".into();
    e.job_id = "fmt".into();
    e.step_key = String::new();
    e.profile_digest = hash(format!("synthetic fmt profile fixture v{version}").as_bytes());
    e.invocations = br#"[{"program":"cargo","args":["fmt","--all","--","--check"],"cwd":".","env":{"RUST_BACKTRACE":"1"}}]"#.to_vec();
    e
}
fn synthetic_content(version: u8) -> BTreeMap<String, Vec<u8>> {
    let raw = match version {
        1 => include_str!("fmt-fixture.json"),
        2 => include_str!("fmt-fixture-v2.json"),
        _ => panic!("unsupported synthetic fixture version"),
    };
    for incidental in ["/workspace/", "/home/", "dev-toolchains", "1.96.1"] {
        assert!(
            !raw.contains(incidental),
            "fixture contains incidental host data"
        );
    }
    let content: BTreeMap<String, String> = serde_json::from_str(raw).unwrap();
    assert!(content["SYNTHETIC-TEST-DATA.txt"].contains("never provider acceptance evidence"));
    content
        .into_iter()
        .map(|(k, v)| (k, v.into_bytes()))
        .collect()
}
fn synthetic_policy(p: MeasuredPreparation<'_>) -> Result<()> {
    ensure(
        p.manifest["synthetic_test_data"] == true
            && p.context["SYNTHETIC_TEST_DATA"] == "offline-parser-only"
            && p.manifest["platform"] == "linux-x86_64"
            && p.context["SOURCE_CI_EVENT"] == "pull_request",
        "synthetic fixture context mismatch",
    )?;
    let programs: BTreeSet<_> = p
        .toolchain
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["program"].as_str())
        .collect();
    ensure(
        programs == BTreeSet::from(["cargo", "rustc", "rustfmt"]),
        "synthetic fixture direct tools mismatch",
    )
}
#[test]
fn synthetic_fmt_fixture_roundtrips_exact_bytes_without_remote_readiness_claim() {
    let files = ArtifactFiles::fixture(SYNTHETIC_ORIGIN, synthetic_content(1));
    let e = synthetic_expected(1);
    let p = verify_preparation(&files, &e, synthetic_policy).unwrap();
    let result = verify_execution(&files, &p).unwrap();
    assert_eq!(
        result.invocation_digest(),
        "sha256:ff14fcc9c5341e8b63c0ef630ce13c4285181ba9c1b4b6048dafdce1bbdbe1a6"
    );
}

#[test]
fn synthetic_v2_fmt_closure_rows_are_separate_from_version_probes() {
    let files = ArtifactFiles::fixture(SYNTHETIC_ORIGIN, synthetic_content(2));
    let e = synthetic_expected(2);
    let p = verify_preparation(&files, &e, synthetic_policy).unwrap();
    let result = verify_execution(&files, &p).unwrap();
    assert_eq!(
        result.invocation_digest(),
        "sha256:ff14fcc9c5341e8b63c0ef630ce13c4285181ba9c1b4b6048dafdce1bbdbe1a6"
    );
}
#[test]
fn final_revision_is_bound_to_finite_checkout_or_artifact_recipe() {
    let mut e = expected();
    e.execution_revision = "e".repeat(40);
    assert!(final_revision_matches(&e, &json!(e.execution_revision)));
    assert!(!final_revision_matches(&e, &json!(e.head)));
    assert!(!final_revision_matches(&e, &Value::Null));
    e.workflow = "docs_sync.yml".into();
    e.job = "sync-docs".into();
    e.check = "docs-drift".into();
    assert!(final_revision_matches(&e, &json!(e.head)));
    assert!(!final_revision_matches(&e, &json!(e.execution_revision)));
    e.execution_mode = "authenticated_integration_artifacts".into();
    assert!(final_revision_matches(&e, &Value::Null));
    assert!(!final_revision_matches(&e, &json!(e.head)));
}

#[test]
fn synthetic_static_fixture_digest_chains_still_reject_tampering() {
    for version in [1, 2] {
        let e = synthetic_expected(version);
        for path in [
            "fmt/preparation/tool-0.stdout.log",
            "fmt/preparation/context.json",
        ] {
            let mut f = synthetic_content(version);
            f.insert(path.into(), b"tampered".to_vec());
            assert!(
                verify_preparation(
                    &ArtifactFiles::fixture(SYNTHETIC_ORIGIN, f),
                    &e,
                    synthetic_policy
                )
                .is_err()
            );
        }
        for broken_chain in [false, true] {
            let mut f = synthetic_content(version);
            if broken_chain {
                mutate(&mut f, "fmt/execution/execution.json", |v| {
                    v["results"][0]["previous_record_sha256"] =
                        hash(b"unrelated synthetic record").into();
                });
            } else {
                f.insert(
                    "fmt/execution/command-0.stdout.log".into(),
                    b"tampered".to_vec(),
                );
            }
            let files = ArtifactFiles::fixture(SYNTHETIC_ORIGIN, f);
            let p = verify_preparation(&files, &e, synthetic_policy).unwrap();
            assert!(verify_execution(&files, &p).is_err());
        }
    }
    let mut f = synthetic_content(2);
    f.insert(
        "fmt/preparation/rust-sysroot.stdout.log".into(),
        b"tampered closure probe".to_vec(),
    );
    assert!(
        verify_preparation(
            &ArtifactFiles::fixture(SYNTHETIC_ORIGIN, f),
            &synthetic_expected(2),
            synthetic_policy
        )
        .is_err()
    );
}

fn imported_fixture() -> (BTreeMap<String, Vec<u8>>, SourceExpectation) {
    let mut e = expected();
    e.provider = "buildkite_gha".into();
    e.workflow = ".github/workflows/ci.yml".into();
    e.job = "test".into();
    e.job_id = "44444444-4444-4444-4444-444444444444".into();
    e.step_key = "gha-test".into();
    e.snapshot = "a".repeat(40);
    let mut files = fixture();
    let mut preparation = value(&files["fmt/preparation/preparation.json"]).unwrap();
    for (k, v) in [
        ("provider", e.provider.as_str()),
        ("workflow", e.workflow.as_str()),
        ("job", e.job.as_str()),
        ("job_id", e.job.as_str()),
        ("step_key", ""),
        ("snapshot", e.snapshot.as_str()),
    ] {
        preparation[k] = v.into();
    }
    let prep = bytes(&preparation);
    for path in [
        "fmt/preparation/preparation.json",
        "fmt/execution/preparation.json",
        "fmt/execution/frozen-preparation.json",
    ] {
        files.insert(path.into(), prep.clone());
    }
    let mut execution = value(&files["fmt/execution/execution.json"]).unwrap();
    for k in [
        "provider", "workflow", "job", "job_id", "step_key", "snapshot",
    ] {
        execution[k] = preparation[k].clone();
    }
    execution["preparation_digest"] = hash(&prep).into();
    execution["results"][0]["previous_record_sha256"] = hash(&prep).into();
    execution["terminal_record_sha256"] = hash(&bytes(&execution["results"][0])).into();
    files.insert("fmt/execution/execution.json".into(), bytes(&execution));
    (files, e)
}
fn imported_origin(e: &SourceExpectation) -> String {
    format!("buildkite:{}/jobs/{}/source-proof", e.run_id, e.job_id)
}
#[test]
fn imported_shell_claim_requires_verified_recipe_and_exact_native_artifact() {
    let recipe = super::super::public_buildkite::tests::synthetic_source_recipe();
    let (files, e) = imported_fixture();
    let artifact = ArtifactFiles::fixture(&imported_origin(&e), files);
    assert!(
        verify_preparation(&artifact, &e, policy)
            .unwrap_err()
            .to_string()
            .contains("ImportedRecipe")
    );
    let preparation = verify_imported_preparation(&artifact, &e, &recipe, policy).unwrap();
    let execution = verify_execution(&artifact, &preparation).unwrap();
    assert!(matches!(
        execution.observation(),
        readiness_evidence::Observation::Executed {
            exit_code: Some(0),
            ..
        }
    ));
    assert_eq!(
        preparation.expected().job_id,
        "44444444-4444-4444-4444-444444444444"
    );
    assert_eq!(preparation.expected().step_key, "gha-test");
}
#[test]
fn imported_claim_rejects_swapped_producer_logical_job_build_retry_and_source() {
    let recipe = super::super::public_buildkite::tests::synthetic_source_recipe();
    for mode in 0..9 {
        let (mut files, mut e) = imported_fixture();
        let mut origin = imported_origin(&e);
        match mode {
            0 => {
                origin = origin.replace(
                    "44444444-4444-4444-4444-444444444444",
                    "55555555-5555-5555-5555-555555555555",
                )
            }
            1 => e.job = "different-logical-job".into(),
            2 => e.run_id = "55555555-5555-5555-5555-555555555555".into(),
            3 => e.run_attempt = "1".into(),
            4 => e.job_id = "55555555-5555-5555-5555-555555555555".into(),
            5 => e.step_key = "different-native-key".into(),
            6 => e.snapshot = "f".repeat(40),
            7 => mutate(&mut files, "fmt/preparation/preparation.json", |v| {
                v["run_attempt"] = "1".into()
            }),
            _ => origin = "github:org/repo/runs/7/attempts/1/artifacts/9".into(),
        }
        let artifact = ArtifactFiles::fixture(&origin, files);
        assert!(
            verify_imported_preparation(&artifact, &e, &recipe, policy).is_err(),
            "case {mode}"
        );
    }
}
