use super::*;
use serde_json::{Value, json};
const PIPE: &str = "11111111-2222-3333-4444-555555555555";
fn snapshot() -> snapshot::PreparedSnapshot {
    snapshot::PreparedSnapshot::fixture(
        snapshot::SnapshotRequest {
            repository: Repository::new("org", "repo").unwrap(),
            request_id: PIPE.into(),
            source_ref: "refs/heads/work".into(),
            source_head: "a".repeat(40),
            source_tree: "b".repeat(40),
            accepted_base: "c".repeat(40),
            accepted_base_ref: "refs/heads/master".into(),
            tags: Default::default(),
        },
        "d".repeat(40),
        "/tmp".into(),
    )
}
fn plan() -> SourceRunPlan {
    let workflows = vec![
        ".github/workflows/ci.yml".into(),
        ".github/workflows/security.yml".into(),
    ];
    SourceRunPlan {
        github_workflows: workflows.clone(),
        buildkite_pipelines: vec![ImportedPipeline {
            organization: "org".into(),
            pipeline: "pipe".into(),
            pipeline_id: PIPE.into(),
            workflows,
        }],
    }
}
struct Gh {
    missing: bool,
    running: bool,
    attempt: u32,
    denied: bool,
    urls: Vec<String>,
    calls: usize,
    bad_head: bool,
    failed: bool,
}
impl Default for Gh {
    fn default() -> Self {
        Self {
            missing: false,
            running: false,
            attempt: 1,
            denied: false,
            urls: vec![
                "https://buildkite.com/org/pipe/builds/7".into(),
                "https://buildkite.com/org/pipe/builds/8".into(),
            ],
            calls: 0,
            bad_head: false,
            failed: false,
        }
    }
}
impl Runner for Gh {
    fn execute(&mut self, spec: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
        self.calls += 1;
        if self.denied {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "synthetic denied",
            ));
        }
        if spec.program() == Program::GitTransport {
            return Ok(Reply {
                exit_code: if spec.arguments().iter().any(|s| s == "config") {
                    1
                } else {
                    0
                },
                stdout: vec![],
            });
        }
        spec.authorize_provider_read()?;
        let path = spec.arguments().last().unwrap();
        let s = snapshot();
        let r = s.request();
        let body = if path.contains("/git/commits/") {
            let transport = path.ends_with(s.transport_head());
            json!({"sha":if transport{s.transport_head()}else{&r.source_head},"tree":{"sha":r.source_tree},"parents":if transport{vec![json!({"sha":r.source_head})]}else{vec![]},"message":if transport{r.message().unwrap()}else{"source".into()}})
        } else if path.contains("/git/ref/") {
            let transport = path.contains("decapod/preflight");
            json!({"ref":if transport{r.transport_ref()}else{r.accepted_base_ref.clone()},"object":{"type":"commit","sha":if transport{s.transport_head()}else{&r.accepted_base}}})
        } else if path.contains("/statuses?") {
            json!([])
        } else if path.contains("/check-runs?") {
            let checks:Vec<_>=self.urls.iter().enumerate().map(|(i,url)|json!({"id":i+1,"head_sha":if self.bad_head{"f".repeat(40)}else{s.transport_head().into()},"details_url":url})).collect();
            json!({"total_count":checks.len(),"check_runs":checks})
        } else if path.contains("/runs?") {
            let security = path.contains("/102/");
            let id = if security { 102 } else { 101 };
            let workflow = if security {
                ".github/workflows/security.yml"
            } else {
                ".github/workflows/ci.yml"
            };
            let run = json!({"id":id,"run_attempt":self.attempt,"workflow_id":id,"path":workflow,"head_sha":s.transport_head(),"head_branch":r.transport_ref().trim_start_matches("refs/heads/"),"event":"push","status":if self.running{"in_progress"}else{"completed"},"conclusion":if self.running{None}else if self.failed{Some("failure")}else{Some("success")},"repository":{"id":1,"full_name":"org/repo"},"head_repository":{"id":1,"full_name":"org/repo"}});
            json!({"total_count":if self.missing{0}else{1},"workflow_runs":if self.missing{vec![]}else{vec![run]}})
        } else if path.ends_with("ci.yml") || path.ends_with("security.yml") {
            let security = path.ends_with("security.yml");
            let id = if security { 102 } else { 101 };
            json!({"id":id,"path":if security{".github/workflows/security.yml"}else{".github/workflows/ci.yml"},"state":"active","url":format!("https://api.github.com/repos/org/repo/actions/workflows/{id}")})
        } else {
            panic!("unexpected non-metadata request: {path}")
        };
        Ok(Reply {
            exit_code: 0,
            stdout: serde_json::to_vec(&body).unwrap(),
        })
    }
}
struct Bk {
    envelopes: BTreeMap<u64, Value>,
    calls: usize,
}
impl Default for Bk {
    fn default() -> Self {
        let s = snapshot();
        Self { calls:0, envelopes:[7,8].into_iter().map(|number|(number,json!({
            "id":format!("00000000-0000-0000-0000-{number:012}"),"number":number,"path":format!("/org/pipe/builds/{number}"),
            "commit_id":s.transport_head(),"branch_name":s.request().transport_ref().trim_start_matches("refs/heads/"),
            "github_workflow_path":if number==7{".github/workflows/ci.yml"}else{".github/workflows/security.yml"},"github_webhook_event":"push",
            "state":"passed","blocked_state":null,"account":{"slug":"org"},"project":{"id":PIPE,"slug":"pipe","public":true,"provider":{"id":"github","url":"https://github.com/org/repo"}}
        }))).collect() }
    }
}
impl buildkite::PublicReader for Bk {
    fn get(&mut self, request: &buildkite::PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        self.calls += 1;
        assert_eq!(
            request.authorize_read()?.kind(),
            buildkite::PublicReadKind::BuildEnvelope
        );
        let url = request.url();
        let n = url
            .rsplit('/')
            .next()
            .unwrap()
            .strip_suffix(".json")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        Ok(serde_json::to_vec(self.envelopes.get(&n).unwrap()).unwrap())
    }
}
fn budget(c: &Cancellation) -> Budget<'_> {
    Budget::new(Duration::from_secs(5), 1024 * 1024, c).unwrap()
}
#[test]
fn multiple_imported_workflows_in_one_pipeline_complete_without_artifact_writes() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut gh = Gh::default();
    let mut bk = Bk::default();
    let result = wait_for_source_runs(
        &mut gh,
        &mut bk,
        &snapshot(),
        &plan(),
        Duration::from_secs(1),
        &b,
    )
    .unwrap();
    assert_eq!(
        result.github(".github/workflows/ci.yml").unwrap().run_id(),
        101
    );
    assert_eq!(
        result
            .buildkite("org", "pipe", ".github/workflows/security.yml")
            .unwrap()
            .build()
            .number(),
        8
    );
    result
        .recheck(&mut gh, &mut bk, &snapshot(), &plan(), &b)
        .unwrap();
    assert_eq!(bk.calls, 6); // envelopes only: initial, barrier recheck, caller recheck
}
#[test]
fn missing_and_running_are_pending_then_terminal_without_repeated_receipts() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut gh = Gh {
        missing: true,
        ..Gh::default()
    };
    let mut bk = Bk::default();
    let mut state = PollState::default();
    assert!(!poll(&mut gh, &mut bk, &snapshot(), &plan(), &mut state, &b).unwrap());
    gh.missing = false;
    gh.running = true;
    assert!(!poll(&mut gh, &mut bk, &snapshot(), &plan(), &mut state, &b).unwrap());
    gh.running = false;
    assert!(poll(&mut gh, &mut bk, &snapshot(), &plan(), &mut state, &b).unwrap());
}
#[test]
fn changed_attempt_or_duplicate_workflow_is_never_adopted() {
    for duplicate in [false, true] {
        let c = Cancellation::default();
        let b = budget(&c);
        let mut gh = Gh::default();
        let mut bk = Bk::default();
        let mut state = PollState::default();
        assert!(poll(&mut gh, &mut bk, &snapshot(), &plan(), &mut state, &b).unwrap());
        if duplicate {
            bk.envelopes.get_mut(&8).unwrap()["github_workflow_path"] =
                ".github/workflows/ci.yml".into();
        } else {
            gh.attempt = 2;
        }
        assert!(poll(&mut gh, &mut bk, &snapshot(), &plan(), &mut state, &b).is_err());
    }
}
#[test]
fn foreign_base_source_pipeline_event_and_blocked_pass_reject() {
    for (key, value) in [
        ("commit_id", json!("e".repeat(40))),
        ("branch_name", json!("master")),
        ("github_webhook_event", json!("pull_request")),
        ("blocked_state", json!("blocked")),
        ("state", json!("skipped")),
        ("state", json!("unknown")),
    ] {
        let c = Cancellation::default();
        let b = budget(&c);
        let mut gh = Gh::default();
        let mut bk = Bk::default();
        bk.envelopes.get_mut(&7).unwrap()[key] = value;
        assert!(
            poll(
                &mut gh,
                &mut bk,
                &snapshot(),
                &plan(),
                &mut PollState::default(),
                &b
            )
            .is_err(),
            "{key}"
        );
    }
    let c = Cancellation::default();
    let b = budget(&c);
    let mut gh = Gh {
        bad_head: true,
        ..Gh::default()
    };
    let mut bk = Bk::default();
    assert!(
        poll(
            &mut gh,
            &mut bk,
            &snapshot(),
            &plan(),
            &mut PollState::default(),
            &b
        )
        .is_err()
    );
}
#[test]
fn locator_hosts_paths_queries_and_credentials_cannot_be_followed() {
    let p = &plan().buildkite_pipelines[0];
    for url in [
        "https://evil.invalid/org/pipe/builds/7",
        "https://buildkite.com.evil.invalid/org/pipe/builds/7",
        "https://user@buildkite.com/org/pipe/builds/7",
        "http://buildkite.com/org/pipe/builds/7",
        "https://buildkite.com:443/org/pipe/builds/7",
        "https://buildkite.com/org/pipe/builds/7?secret=x",
        "https://buildkite.com/org/pipe/builds/../7",
        "https://buildkite.com/org/pipe/builds/07",
    ] {
        assert!(build_number(url, p).is_none(), "{url}");
    }
}
#[test]
fn authorization_denial_cancellation_and_deadline_stop_without_retry() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut gh = Gh {
        denied: true,
        ..Gh::default()
    };
    let mut bk = Bk::default();
    let e = wait_for_source_runs(
        &mut gh,
        &mut bk,
        &snapshot(),
        &plan(),
        Duration::from_secs(1),
        &b,
    )
    .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(gh.calls, 1);
    assert_eq!(bk.calls, 0);
    c.cancel();
    gh.denied = false;
    gh.calls = 0;
    assert_eq!(
        wait_for_source_runs(
            &mut gh,
            &mut bk,
            &snapshot(),
            &plan(),
            Duration::from_secs(1),
            &b
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(gh.calls, 0);
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_millis(1), 1024 * 1024, &c).unwrap();
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(
        wait_for_source_runs(
            &mut gh,
            &mut bk,
            &snapshot(),
            &plan(),
            Duration::from_secs(1),
            &b
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn terminal_failure_diagnostics_identify_verified_workflow_and_provider_target() {
    let c = Cancellation::default();
    let b = budget(&c);
    let mut gh = Gh {
        failed: true,
        ..Gh::default()
    };
    let mut bk = Bk::default();
    let error = poll(
        &mut gh,
        &mut bk,
        &snapshot(),
        &plan(),
        &mut PollState::default(),
        &b,
    )
    .unwrap_err()
    .to_string();
    for part in [
        "completed without success",
        "workflow=.github/workflows/ci.yml",
        "run=101",
        "attempt=1",
        "status=completed",
        "conclusion=failure",
        "https://github.com/org/repo/actions/runs/101/attempts/1",
    ] {
        assert!(error.contains(part), "{part}: {error}");
    }
    gh.failed = false;
    for blocked in [false, true] {
        bk.envelopes.get_mut(&7).unwrap()["state"] = if blocked {
            "passed".into()
        } else {
            "failed".into()
        };
        bk.envelopes.get_mut(&7).unwrap()["blocked_state"] = if blocked {
            "blocked".into()
        } else {
            Value::Null
        };
        let error = poll(
            &mut gh,
            &mut bk,
            &snapshot(),
            &plan(),
            &mut PollState::default(),
            &b,
        )
        .unwrap_err()
        .to_string();
        for part in [
            "workflow=.github/workflows/ci.yml",
            "build=7",
            "uuid=00000000-0000-0000-0000-000000000007",
            "https://buildkite.com/org/pipe/builds/7",
        ] {
            assert!(error.contains(part), "{part}: {error}");
        }
        assert!(error.contains(if blocked {
            "state=passed blocked=true"
        } else {
            "state=failed blocked=false"
        }));
    }
}
