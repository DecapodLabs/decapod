use super::*;
use serde_json::{Value, json};
fn snapshot() -> PreparedSnapshot {
    PreparedSnapshot::fixture(
        super::super::snapshot::SnapshotRequest {
            repository: Repository::new("owner", "repo").unwrap(),
            request_id: "11111111-2222-3333-4444-555555555555".into(),
            source_ref: "refs/heads/work".into(),
            source_head: "a".repeat(40),
            source_tree: "b".repeat(40),
            accepted_base: "c".repeat(40),
            accepted_base_ref: "refs/heads/master".into(),
            tags: Default::default(),
        },
        "d".repeat(40),
        PathBuf::from("/tmp"),
    )
}
fn workflow() -> GithubWorkflow {
    GithubWorkflow {
        id: 123,
        path: ".github/workflows/ci.yml".into(),
        jobs: vec![GithubJob {
            key: "gh:test".into(),
            contract: GithubExecutionContract {
                schema_version: GITHUB_EXECUTION_SCHEMA_VERSION.into(),
                repository: "owner/repo".into(),
                workflow: ".github/workflows/ci.yml".into(),
                job: "test".into(),
                provider_job_name: "test (linux)".into(),
                matrix: BTreeMap::from([("os".into(), "linux".into())]),
                profile: "source".into(),
                source_definition_digest: hash(b"workflow job bytes"),
                source_head: "a".repeat(40),
                accepted_base_revision: "c".repeat(40),
                transport_revision: "d".repeat(40),
                source_tree: "b".repeat(40),
                required_steps: vec!["Verify immutable context", "Run substantive tests"]
                    .into_iter()
                    .map(
                        |name| super::super::readiness_evidence::GithubRequiredStep {
                            name: name.into(),
                            definition_digest: hash(name.as_bytes()),
                            failure_mode: GithubFailureMode::RequiredTerminalSuccess,
                        },
                    )
                    .collect(),
            },
        }],
    }
}
fn pipeline() -> BuildkitePipeline {
    BuildkitePipeline {
        organization: "org".into(),
        slug: "pipeline".into(),
        id: "11111111-1111-1111-1111-111111111111".into(),
        source: BuildkiteSource::Webhook,
        jobs: vec![BuildkiteJob {
            key: "bk:test".into(),
            step_key: "test".into(),
            parallel_index: None,
            parallel_total: None,
            matrix: None,
            command_digest: hash(b"cargo test --locked"),
        }],
    }
}
fn run() -> Value {
    let s = snapshot();
    json!({"id":42,"run_attempt":1,"workflow_id":123,"path":".github/workflows/ci.yml","head_sha":s.transport_head(),"head_branch":s.request().transport_ref().strip_prefix("refs/heads/").unwrap(),"event":"push","status":"completed","conclusion":"success","repository":{"full_name":"owner/repo"},"head_repository":{"full_name":"owner/repo"}})
}
fn jobs() -> Value {
    json!({"total_count":1,"jobs":[{"id":99,"run_id":42,"run_attempt":1,"head_sha":"d".repeat(40),"name":"test (linux)","status":"completed","conclusion":"success","steps":[{"name":"Verify immutable context","number":1,"status":"completed","conclusion":"success"},{"name":"Run substantive tests","number":2,"status":"completed","conclusion":"success"}]}]})
}
fn build() -> Value {
    let s = snapshot();
    let p = pipeline();
    json!({"id":"22222222-2222-2222-2222-222222222222","number":7,"commit":s.transport_head(),"branch":s.request().transport_ref().strip_prefix("refs/heads/").unwrap(),"source":"webhook","state":"passed","blocked":false,"pipeline":{"id":p.id,"slug":p.slug,"repository":"git@github.com:owner/repo.git","url":"https://api.buildkite.com/v2/organizations/org/pipelines/pipeline"},"job_state_counts":{"total":1,"states":{"passed":1}},"jobs":[{"id":"33333333-3333-3333-3333-333333333333","type":"script","step_key":"test","state":"passed","exit_status":0,"soft_failed":false,"command":"cargo test --locked","retried":false}]})
}
struct Mock {
    run: Value,
    jobs: Value,
    build: Value,
    calls: Vec<Vec<String>>,
    base_changed: bool,
    missing_runs: bool,
    failed: bool,
}
impl Default for Mock {
    fn default() -> Self {
        Self {
            run: run(),
            jobs: jobs(),
            build: build(),
            calls: vec![],
            base_changed: false,
            missing_runs: false,
            failed: false,
        }
    }
}
impl Runner for Mock {
    fn execute(&mut self, c: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
        self.calls.push(c.args.clone());
        if self.failed {
            return Ok(Reply {
                exit_code: 1,
                stdout: vec![],
            });
        }
        if matches!(
            c.provider_request(),
            Some(ProviderRequest::GithubJobLog { .. })
        ) {
            return Ok(Reply {
                exit_code: 0,
                stdout: b"Actual provider job log bytes\n".to_vec(),
            });
        }
        if c.program == Program::GitTransport && c.args.iter().any(|v| v == "config") {
            return Ok(Reply {
                exit_code: 1,
                stdout: vec![],
            });
        }
        if c.program == Program::GitTransport && c.args.iter().any(|v| v == "--tags") {
            return Ok(Reply {
                exit_code: 0,
                stdout: vec![],
            });
        }
        let endpoint = c.args.last().unwrap();
        if endpoint.ends_with("/log") {
            let content = "Actual Buildkite job log bytes\n";
            return Ok(Reply{exit_code:0,stdout:serde_json::to_vec(&json!({"url":format!("https://api.buildkite.com/v2/organizations/org{endpoint}"),"content":content,"size":content.len()})).unwrap()});
        }

        let s = snapshot();
        let r = s.request();
        let v = if endpoint.contains("/git/commits/") {
            let transport = endpoint.ends_with(s.transport_head());
            json!({"sha":if transport{s.transport_head()}else{r.source_head.as_str()},"tree":{"sha":r.source_tree},"parents":if transport{vec![json!({"sha":r.source_head})]}else{vec![]},"message":if transport{r.message().unwrap()}else{"source".into()}})
        } else if endpoint.contains("/git/ref/") {
            let temp = endpoint.contains("decapod/preflight");
            json!({"ref":if temp{r.transport_ref()}else{r.accepted_base_ref.clone()},"object":{"type":"commit","sha":if temp{s.transport_head().to_string()}else if self.base_changed{"e".repeat(40)}else{r.accepted_base.clone()}}})
        } else if endpoint.contains("/attempts/") {
            self.jobs.clone()
        } else if endpoint.contains("/actions/workflows/") {
            json!({"total_count":if self.missing_runs{0}else{1},"workflow_runs":if self.missing_runs{vec![]}else{vec![self.run.clone()]}})
        } else if endpoint.contains("/actions/runs/") {
            self.run.clone()
        } else if endpoint.contains("/builds?") {
            json!([self.build.clone()])
        } else if endpoint.contains("/builds/7?") {
            self.build.clone()
        } else {
            panic!("unexpected endpoint {endpoint}")
        };
        Ok(Reply {
            exit_code: 0,
            stdout: serde_json::to_vec(&v).unwrap(),
        })
    }
}
fn budget(c: &Cancellation) -> Budget<'_> {
    Budget::new(Duration::from_secs(10), 1024 * 1024, c).unwrap()
}
#[test]
fn complete_mixed_suite_binds_exact_context_and_jobs() {
    let c = Cancellation::default();
    let mut m = Mock::default();
    let plan = RemotePlan {
        github: vec![workflow()],
        buildkite: vec![pipeline()],
    };
    let result = collect(
        &mut m,
        &snapshot(),
        &plan,
        Duration::from_secs(1),
        &budget(&c),
    )
    .unwrap();
    assert!(result.complete && result.all_passed);
    assert_eq!(result.rows.len(), 2);
    assert!(result.rows.iter().all(|v| hex_digest(v.response_digest())));
    assert!(
        m.calls
            .iter()
            .all(|a| !a.contains(&"POST".into()) && !a.contains(&"push".into()))
    );
}
#[test]
fn required_skip_is_never_green() {
    for skip in ["skipped", "neutral"] {
        let mut m = Mock::default();
        m.jobs["jobs"][0]["steps"][1]["conclusion"] = json!(skip);
        let c = Cancellation::default();
        let r = collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![],
            },
            Duration::from_secs(1),
            &budget(&c),
        )
        .unwrap();
        assert!(!r.all_passed);
        assert_eq!(r.rows[0].state(), &State::Skipped);
    }
}
#[test]
fn missing_substantive_step_fails_even_with_green_job() {
    let mut m = Mock::default();
    m.jobs["jobs"][0]["steps"] = json!([]);
    let c = Cancellation::default();
    assert!(
        collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
}
#[test]
fn provider_projection_cannot_rebind_event_workflow_head_or_attempt() {
    for (field, v) in [
        ("event", json!("pull_request")),
        ("path", json!(".github/workflows/other.yml")),
        ("head_sha", json!("e".repeat(40))),
        ("head_branch", json!("work")),
        ("workflow_id", json!(999)),
    ] {
        let mut raw = run();
        raw[field] = v;
        let decoded: GhRun = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(check_run(&decoded, &snapshot(), &workflow()).is_err());
    }
}
#[test]
fn foreign_job_attempt_and_duplicate_matrix_names_rejected() {
    for duplicate in [false, true] {
        let mut m = Mock::default();
        if duplicate {
            let j = m.jobs["jobs"][0].clone();
            m.jobs["jobs"].as_array_mut().unwrap().push(j);
            m.jobs["total_count"] = json!(2);
        } else {
            m.jobs["jobs"][0]["run_attempt"] = json!(2);
        }
        let c = Cancellation::default();
        let r: GhRun = decode(&serde_json::to_vec(&run()).unwrap()).unwrap();
        assert!(collect_gh(&mut m, &snapshot(), &workflow(), &r, &budget(&c)).is_err());
    }
}
#[test]
fn blocked_passed_build_is_not_terminal_or_green() {
    let mut v = build();
    v["blocked"] = json!(true);
    let b: BkBuild = decode(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(collect_bk(&b, "0".repeat(64), &snapshot(), &pipeline()).is_err());
}
#[test]
fn buildkite_script_requires_real_zero_exit_and_no_soft_fail() {
    for (field, v) in [
        ("exit_status", json!(1)),
        ("exit_status", Value::Null),
        ("soft_failed", json!(true)),
        ("soft_failed", Value::Null),
    ] {
        let mut raw = build();
        raw["jobs"][0][field] = v;
        let b: BkBuild = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let (rows, done) = collect_bk(&b, "0".repeat(64), &snapshot(), &pipeline()).unwrap();
        assert!(done);
        assert_eq!(rows[0].state(), &State::Failure);
    }
}
#[test]
fn buildkite_waiter_or_changed_command_or_partial_inventory_rejected() {
    for (field, v) in [
        ("type", json!("waiter")),
        ("command", json!("true")),
        ("retried", json!(true)),
    ] {
        let mut raw = build();
        raw["jobs"][0][field] = v;
        let b: BkBuild = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(collect_bk(&b, "0".repeat(64), &snapshot(), &pipeline()).is_err());
    }
    let mut raw = build();
    raw["job_state_counts"]["total"] = json!(2);
    let b: BkBuild = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(collect_bk(&b, "0".repeat(64), &snapshot(), &pipeline()).is_err());
}
#[test]
fn moved_accepted_target_fails_before_jobs() {
    let mut m = Mock {
        base_changed: true,
        ..Default::default()
    };
    let c = Cancellation::default();
    assert!(
        collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
    assert!(
        m.calls
            .iter()
            .all(|args| !args.iter().any(|value| value.contains("actions/")))
    );
}
#[test]
fn unauthenticated_or_missing_run_cannot_mint_success() {
    let c = Cancellation::default();
    let mut m = Mock {
        failed: true,
        ..Default::default()
    };
    assert!(
        collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
    let mut m = Mock {
        missing_runs: true,
        ..Default::default()
    };
    let b = Budget::new(Duration::from_millis(20), 1024 * 1024, &c).unwrap();
    assert_eq!(
        collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &b
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
}
#[test]
fn pre_cancelled_collector_spawns_nothing() {
    let c = Cancellation::default();
    c.cancel();
    let mut m = Mock::default();
    assert!(
        collect(
            &mut m,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
    assert!(m.calls.is_empty());
}
#[test]
fn unknown_states_are_not_skipped_success() {
    assert!(gh_state("mystery", None).is_err());
    assert!(bk_state("unblocked").is_err());
}
#[test]
fn input_validation_rejects_api_route_injection_and_empty_steps() {
    let mut p = pipeline();
    p.organization = "org/other".into();
    assert!(
        RemotePlan {
            github: vec![],
            buildkite: vec![p]
        }
        .validate()
        .is_err()
    );
    let mut w = workflow();
    w.jobs[0].contract.required_steps.clear();
    assert!(
        RemotePlan {
            github: vec![w],
            buildkite: vec![]
        }
        .validate()
        .is_err()
    );
}
#[test]
fn duplicate_json_fields_are_rejected() {
    let raw = serde_json::to_string(&run()).unwrap();
    let raw = raw.replacen('{', "{\"id\":999,", 1);
    assert!(decode::<GhRun>(raw.as_bytes()).is_err());
}
#[test]
fn malformed_bindings_and_zero_hashes_rejected() {
    assert!(Repository::new("--host", "repo").is_err());
    assert!(Repository::new("owner", "repo?x").is_err());
    assert!(!oid(&"0".repeat(40)));
    let mut m = Mock::default();
    m.build["pipeline"]["url"] =
        json!("https://api.buildkite.com/v2/organizations/other/pipelines/pipeline");
    let decoded: BkBuild = decode(&serde_json::to_vec(&m.build).unwrap()).unwrap();
    assert!(check_build(&decoded, &snapshot(), &pipeline()).is_err());
}

#[test]
fn attestation_contains_measured_logs_and_never_an_invented_exit() {
    let c = Cancellation::default();
    let mut mock = Mock::default();
    let result = collect(
        &mut mock,
        &snapshot(),
        &RemotePlan {
            github: vec![workflow()],
            buildkite: vec![pipeline()],
        },
        Duration::from_secs(1),
        &budget(&c),
    )
    .unwrap();
    let gh = result
        .rows
        .iter()
        .find(|row| row.key() == "gh:test")
        .unwrap();
    assert_eq!(gh.exit_code(), None);
    let readiness_evidence::Observation::GithubActions { attestation } = gh.observation().unwrap()
    else {
        panic!("wrong observation")
    };
    assert_eq!(
        attestation.log_digest,
        hash(b"Actual provider job log bytes\n")
    );
    assert_ne!(attestation.log_digest, attestation.response_digest);
    assert_eq!(attestation.steps.len(), 2);
    assert_eq!(attestation.run_id, 42);
    assert_eq!(attestation.run_attempt, 1);
    assert_eq!(attestation.job_id, 99);
    assert!(attestation.artifact_digest.is_none());
    let bk = result
        .rows
        .iter()
        .find(|row| row.key() == "bk:test")
        .unwrap();
    let readiness_evidence::Observation::Executed {
        exit_code,
        output_digest,
        artifact_digest,
        ..
    } = bk.observation().unwrap()
    else {
        panic!("wrong numeric observation")
    };
    assert_eq!(exit_code, Some(0));
    assert_eq!(output_digest, hash(b"Actual Buildkite job log bytes\n"));
    assert!(artifact_digest.is_none());
}
#[test]
fn masking_semantics_and_reordered_steps_are_unsupported() {
    for mode in [
        GithubFailureMode::ContinueOnError,
        GithubFailureMode::Unknown,
    ] {
        let mut w = workflow();
        w.jobs[0].contract.required_steps[1].failure_mode = mode;
        assert!(
            RemotePlan {
                github: vec![w],
                buildkite: vec![]
            }
            .validate()
            .is_err()
        );
    }
    let mut mock = Mock::default();
    mock.jobs["jobs"][0]["steps"][0]["name"] = json!("Run substantive tests");
    mock.jobs["jobs"][0]["steps"][1]["name"] = json!("Verify immutable context");
    let c = Cancellation::default();
    assert!(
        collect(
            &mut mock,
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
}
#[test]
fn source_contract_cannot_be_rebound_after_planning() {
    let mut w = workflow();
    w.jobs[0].contract.source_tree = "e".repeat(40);
    let mut mock = Mock::default();
    let c = Cancellation::default();
    assert!(
        collect(
            &mut mock,
            &snapshot(),
            &RemotePlan {
                github: vec![w],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
    assert!(mock.calls.is_empty());
}
#[test]
fn exact_job_logs_must_be_retrieved() {
    struct NoLogs(Mock);
    impl Runner for NoLogs {
        fn execute(&mut self, c: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
            if matches!(
                c.provider_request(),
                Some(ProviderRequest::GithubJobLog { .. })
            ) {
                Ok(Reply {
                    exit_code: 1,
                    stdout: vec![],
                })
            } else {
                self.0.execute(c, b)
            }
        }
    }
    let c = Cancellation::default();
    assert!(
        collect(
            &mut NoLogs(Mock::default()),
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
}
#[test]
fn rerun_during_collection_cannot_reuse_an_earlier_attempt() {
    struct Rerun(Mock);
    impl Runner for Rerun {
        fn execute(&mut self, c: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
            if c.args
                .last()
                .is_some_and(|v| v.ends_with("actions/runs/42"))
            {
                let mut v = run();
                v["run_attempt"] = json!(2);
                Ok(Reply {
                    exit_code: 0,
                    stdout: serde_json::to_vec(&v).unwrap(),
                })
            } else {
                self.0.execute(c, b)
            }
        }
    }
    let c = Cancellation::default();
    assert!(
        collect(
            &mut Rerun(Mock::default()),
            &snapshot(),
            &RemotePlan {
                github: vec![workflow()],
                buildkite: vec![]
            },
            Duration::from_secs(1),
            &budget(&c)
        )
        .is_err()
    );
}
#[test]
fn running_workflow_is_polled_until_terminal() {
    struct Running {
        mock: Mock,
        first: bool,
    }
    impl Runner for Running {
        fn execute(&mut self, c: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
            if self.first
                && c.args
                    .last()
                    .is_some_and(|v| v.contains("actions/workflows/"))
            {
                self.first = false;
                let mut v = run();
                v["status"] = json!("in_progress");
                v["conclusion"] = Value::Null;
                Ok(Reply {
                    exit_code: 0,
                    stdout: serde_json::to_vec(&json!({"total_count":1,"workflow_runs":[v]}))
                        .unwrap(),
                })
            } else {
                self.mock.execute(c, b)
            }
        }
    }
    let c = Cancellation::default();
    let start = Instant::now();
    let result = collect(
        &mut Running {
            mock: Mock::default(),
            first: true,
        },
        &snapshot(),
        &RemotePlan {
            github: vec![workflow()],
            buildkite: vec![],
        },
        Duration::from_secs(1),
        &budget(&c),
    )
    .unwrap();
    assert!(result.complete && result.all_passed);
    assert!(start.elapsed() >= Duration::from_secs(1));
}

#[test]
fn imported_workflow_cannot_enter_single_native_pipeline_collector() {
    let mut value = build();
    value["github_workflow_path"] = ".github/workflows/ci.yml".into();
    let observed: BkBuild = serde_json::from_value(value).unwrap();
    assert!(check_build(&observed, &snapshot(), &pipeline()).is_err());
}
