//! Controlled provider transports exercise the actual SnapshotHost orchestration.
//! Local Git capture, snapshot creation and exact S/H/T/ref verification are real.
//! Metadata and custody bytes below are fixtures, never a live provider request or
//! a replacement for the independently complete source catalog/custody adapters.
use super::tests::{command, project};
use super::*;
use provider::{Budget, CommandSpec, OfficialCli, Program, ProviderRequest, Reply};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

const PIPELINE: &str = "01a0bcf1-5e16-44ae-8457-04495fae8ff5";
const JOB: &str = "33333333-3333-3333-3333-333333333333";
const STEP: &str = "44444444-4444-4444-4444-444444444444";
const INVALID_ZIP: &[u8] = b"fixture bytes that cannot authenticate source execution";

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Progress,
    GithubTerminal(&'static str),
    BuildkiteTerminal(&'static str),
    MissingGithub,
    MissingBuildkite,
    DeniedMetadata,
    CancelMetadata,
    CorruptArchive,
}
struct State {
    scenario: Scenario,
    cancel: Cancellation,
    source_head: String,
    source_tree: String,
    base: String,
    published: Option<(String, String)>,
    github: Vec<String>,
    buildkite: Vec<String>,
    cycle: usize,
    calls: BTreeMap<String, usize>,
    events: Vec<String>,
}
impl State {
    fn record(&mut self, key: String) {
        *self.calls.entry(key.clone()).or_default() += 1;
        self.events.push(key);
    }
    fn count(&self, key: &str) -> usize {
        self.calls.get(key).copied().unwrap_or(0)
    }
    fn snapshot(&self) -> (&str, &str) {
        let (head, reference) = self
            .published
            .as_ref()
            .expect("publish precedes provider reads");
        (head, reference)
    }
    fn github_status(&self) -> (&'static str, Option<&'static str>) {
        match self.scenario {
            Scenario::Progress if self.cycle == 1 => ("queued", None),
            Scenario::Progress if self.cycle == 2 => ("in_progress", None),
            Scenario::GithubTerminal(conclusion) => ("completed", Some(conclusion)),
            _ => ("completed", Some("success")),
        }
    }
    fn buildkite_status(&self) -> &'static str {
        match self.scenario {
            Scenario::Progress if self.cycle == 1 => "scheduled",
            Scenario::Progress if self.cycle == 2 => "running",
            Scenario::BuildkiteTerminal(state) => state,
            _ => "passed",
        }
    }
    fn github_run(&self, index: usize) -> Value {
        let (head, reference) = self.snapshot();
        let (status, conclusion) = self.github_status();
        json!({"id":1000 + index,"run_attempt":1,"workflow_id":100 + index,
            "path":self.github[index],"head_sha":head,
            "head_branch":reference.trim_start_matches("refs/heads/"),
            "event":"push","status":status,"conclusion":conclusion,
            "repository":{"id":1,"full_name":"fixture/decapod"},
            "head_repository":{"id":1,"full_name":"fixture/decapod"}})
    }
    fn github_artifact(&self, index: usize) -> Value {
        let (head, reference) = self.snapshot();
        json!({"id":700,"node_id":"fixture-artifact-700",
            "name":source::custody::artifact_name("fmt", None, "1").unwrap(),
            "size_in_bytes":INVALID_ZIP.len(),
            "url":"https://api.github.com/repos/fixture/decapod/actions/artifacts/700",
            "archive_download_url":"https://api.github.com/repos/fixture/decapod/actions/artifacts/700/zip",
            "expired":false,"digest":hash(INVALID_ZIP),
            "workflow_run":{"id":1000 + index,"repository_id":1,"head_repository_id":1,
                "head_branch":reference.trim_start_matches("refs/heads/"),"head_sha":head}})
    }
}

struct FixtureRunner {
    official: OfficialCli,
    root: PathBuf,
    state: Rc<RefCell<State>>,
}
impl Runner for FixtureRunner {
    fn execute(&mut self, spec: &CommandSpec, budget: &Budget<'_>) -> io::Result<Reply> {
        budget.remaining()?;
        if spec.program() == Program::GitLocal
            || (spec.program() == Program::GitTransport
                && spec.arguments().iter().any(|arg| arg == "config"))
        {
            // Only actual local Git/configuration commands reach a process.
            return self.official.execute(spec, budget);
        }
        if spec.program() == Program::GitTransport {
            let mut state = self.state.borrow_mut();
            assert!(
                spec.arguments()
                    .iter()
                    .any(|arg| arg == "https://github.com/fixture/decapod.git")
            );
            let stdout = if spec.arguments().iter().any(|arg| arg == "push") {
                assert!(state.published.is_none(), "one snapshot submission only");
                let (head, reference) = spec.arguments().last().unwrap().split_once(':').unwrap();
                assert!(reference.starts_with(transport::PREFIX));
                assert!(
                    spec.arguments()
                        .contains(&format!("--force-with-lease={reference}:"))
                );
                assert_eq!(
                    command(&self.root, &["show", "-s", "--format=%P", head]),
                    state.source_head
                );
                assert_eq!(
                    command(&self.root, &["rev-parse", &format!("{head}^{{tree}}")]),
                    state.source_tree
                );
                state.published = Some((head.into(), reference.into()));
                state.record("publish".into());
                Vec::new()
            } else {
                assert!(spec.arguments().iter().any(|arg| arg == "ls-remote"));
                if spec.arguments().iter().any(|arg| arg == "--tags") {
                    state.record("remote-tags".into());
                    Vec::new()
                } else {
                    let reference = spec.arguments().last().unwrap();
                    if reference == "refs/heads/master" {
                        state.record("accepted-target".into());
                        format!("{}\t{reference}\n", state.base).into_bytes()
                    } else if let Some((head, published_ref)) = &state.published {
                        assert_eq!(reference, published_ref);
                        let stdout = format!("{head}\t{reference}\n").into_bytes();
                        state.record("snapshot-exact-readback".into());
                        stdout
                    } else {
                        assert!(reference.starts_with(transport::PREFIX));
                        Vec::new()
                    }
                }
            };
            return Ok(Reply {
                exit_code: 0,
                stdout,
            });
        }
        // Every provider call goes through the production typed read seal.
        spec.authorize_provider_read()?;
        match spec
            .provider_request()
            .expect("typed fixture provider request")
        {
            ProviderRequest::GithubArtifactZip { artifact_id, .. } => {
                assert_eq!(*artifact_id, 700);
                let mut state = self.state.borrow_mut();
                assert!(matches!(state.scenario, Scenario::CorruptArchive));
                state.record("github-zip:700".into());
                Ok(Reply {
                    exit_code: 0,
                    stdout: INVALID_ZIP.to_vec(),
                })
            }
            ProviderRequest::GithubGet { path } => {
                let body = self.github_get(path)?;
                Ok(Reply {
                    exit_code: 0,
                    stdout: serde_json::to_vec(&body).unwrap(),
                })
            }
            _ => panic!("unexpected fixture provider capability"),
        }
    }
}
impl FixtureRunner {
    fn github_get(&self, path: &str) -> io::Result<Value> {
        let endpoint = path.strip_prefix("repos/fixture/decapod/").unwrap();
        let mut state = self.state.borrow_mut();
        if let Some(head) = endpoint.strip_prefix("git/commits/") {
            let (snapshot, _) = state.snapshot();
            assert!([snapshot, state.source_head.as_str()].contains(&head));
            // The returned metadata is derived from the real locally created
            // source/snapshot object, not a made-up S/H/T tuple.
            let raw = command(&self.root, &["cat-file", "commit", head]);
            let (headers, message) = raw.split_once("\n\n").unwrap();
            return Ok(json!({"sha":head,
                "tree":{"sha":headers.lines().find_map(|line| line.strip_prefix("tree ")).unwrap()},
                "parents":headers.lines().filter_map(|line| line.strip_prefix("parent ")).map(|sha|json!({"sha":sha})).collect::<Vec<_>>(),
                "message":message}));
        }
        if let Some(reference) = endpoint.strip_prefix("git/ref/") {
            let reference = format!("refs/{reference}");
            let (snapshot, transport_ref) = state.snapshot();
            let head = if reference == "refs/heads/master" {
                state.base.as_str()
            } else {
                assert_eq!(reference, transport_ref);
                snapshot
            };
            return Ok(json!({"ref":reference,"object":{"sha":head,"type":"commit"}}));
        }
        if endpoint.contains("/statuses?") {
            return Ok(json!([]));
        }
        if endpoint.contains("/check-runs?") {
            let (snapshot, _) = state.snapshot();
            let checks: Vec<_> = state.buildkite.iter().enumerate().filter(|(i, _)|
                !matches!(state.scenario, Scenario::MissingBuildkite) || *i != 0
            ).map(|(i, _)| json!({"id":i + 1,"head_sha":snapshot,
                "details_url":format!("https://buildkite.com/decapodlabs/decapod/builds/{}", i + 1)})).collect();
            return Ok(json!({"total_count":checks.len(),"check_runs":checks}));
        }
        if let Some(workflow) = endpoint.strip_prefix("actions/workflows/") {
            if let Some((id, _)) = workflow.split_once("/runs?") {
                let index = id.parse::<usize>().unwrap() - 100;
                if index == 0 {
                    state.cycle += 1;
                }
                let workflow = state.github[index].clone();
                state.record(format!("github-run:{workflow}"));
                if matches!(state.scenario, Scenario::DeniedMetadata) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "fixture metadata denied",
                    ));
                }
                if matches!(state.scenario, Scenario::CancelMetadata) {
                    state.cancel.cancel();
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "fixture cancellation",
                    ));
                }
                if matches!(state.scenario, Scenario::MissingGithub) && index == 0 {
                    return Ok(json!({"total_count":0,"workflow_runs":[]}));
                }
                let (status, _) = state.github_status();
                state.record(format!("github-state:{status}"));
                return Ok(json!({"total_count":1,"workflow_runs":[state.github_run(index)]}));
            }
            let index = state
                .github
                .iter()
                .position(|path| path.ends_with(&format!("/{workflow}")))
                .unwrap();
            return Ok(
                json!({"id":100 + index,"path":state.github[index],"state":"active",
                "url":format!("https://api.github.com/repos/fixture/decapod/actions/workflows/{}", 100 + index)}),
            );
        }
        if endpoint == "actions/artifacts/700" {
            let index = state
                .github
                .iter()
                .position(|workflow| workflow == ".github/workflows/ci.yml")
                .unwrap();
            return Ok(state.github_artifact(index));
        }
        if let Some(run) = endpoint.strip_prefix("actions/runs/") {
            let (id, suffix) = run.split_once('/').unwrap_or((run, ""));
            let index = id.parse::<usize>().unwrap() - 1000;
            let workflow = state.github[index].clone();
            if suffix.is_empty() {
                return Ok(state.github_run(index));
            }
            assert_eq!(
                state.github_status(),
                ("completed", Some("success")),
                "custody must await terminal metadata"
            );
            if suffix.starts_with("attempts/1/jobs?") {
                state.record(format!("github-jobs:{workflow}"));
                let jobs = if workflow == ".github/workflows/ci.yml" {
                    vec![
                        json!({"id":2000 + index,"run_id":1000 + index,"run_attempt":1,
                        "head_sha":state.snapshot().0,"name":"Lint (fmt)","status":"completed","conclusion":"success",
                        "steps":[{"number":1,"name":source::custody::UPLOAD_STEP_NAME,"status":"completed","conclusion":"success"}]}),
                    ]
                } else {
                    vec![]
                };
                return Ok(json!({"total_count":jobs.len(),"jobs":jobs}));
            }
            if suffix.starts_with("artifacts?") {
                state.record(format!("github-artifacts:{workflow}"));
                let artifacts = if matches!(state.scenario, Scenario::CorruptArchive)
                    && workflow == ".github/workflows/ci.yml"
                {
                    vec![state.github_artifact(index)]
                } else {
                    vec![]
                };
                return Ok(json!({"total_count":artifacts.len(),"artifacts":artifacts}));
            }
        }
        panic!("unexpected fixture GitHub endpoint: {endpoint}");
    }
}

struct FixtureBuildkite(Rc<RefCell<State>>);
impl provider::public_buildkite::PublicReader for FixtureBuildkite {
    fn get(
        &mut self,
        request: &provider::public_buildkite::PublicRequest,
        budget: &Budget<'_>,
    ) -> io::Result<Vec<u8>> {
        budget.remaining()?;
        use provider::public_buildkite::PublicReadKind;
        let seal = request.authorize_read()?;
        let parts: Vec<_> = request.url().split('/').map(str::to_owned).collect();
        let position = parts.iter().position(|s| s == "builds").unwrap();
        let number: usize = parts[position + 1]
            .trim_end_matches(".json")
            .parse()
            .unwrap();
        let mut state = self.0.borrow_mut();
        let workflow = state.buildkite[number - 1].clone();
        let base = format!("/decapodlabs/decapod/builds/{number}");
        let body = match seal.kind() {
            PublicReadKind::BuildEnvelope => {
                let status = state.buildkite_status();
                state.record(format!("buildkite-state:{status}"));
                let (head, reference) = state.snapshot();
                json!({"id":format!("00000000-0000-0000-0000-{number:012}"),"number":number,"path":base,
                    "commit_id":head,"branch_name":reference.trim_start_matches("refs/heads/"),
                    "github_workflow_path":workflow,"github_webhook_event":"push","state":status,"blocked_state":null,
                    "account":{"slug":"decapodlabs"},"project":{"id":PIPELINE,"slug":"decapod","public":true,
                        "provider":{"id":"github","url":"https://github.com/fixture/decapod"}},
                    "statistics":{"jobs_count":1,"steps_count":1}})
            }
            PublicReadKind::FullSteps => {
                assert_eq!(
                    state.buildkite_status(),
                    "passed",
                    "custody must await terminal metadata"
                );
                state.record(format!("buildkite-steps:{workflow}"));
                json!([{"uuid":STEP,"key":"fixture-source","type":"command","command_script":null,
                    "uploaded_by_step_uuid":null,"statistics":{"retries_count":0,"jobs_count":1,
                        "step_children_count":0,"latest_job_id":JOB}}])
            }
            PublicReadKind::FullJobs => {
                state.record(format!("buildkite-jobs:{workflow}"));
                json!({"has_next_page":false,"has_previous_page":false,"records":[{
                    "id":JOB,"step_uuid":STEP,"step_key":"fixture-source","command":null,"matrix":null,
                    "parallel_group_index":null,"parallel_group_total":null,"build_number":number,
                    "base_path":format!("/organizations/decapodlabs/pipelines/decapod/builds/{number}/jobs/{JOB}"),
                    "type":"script","retried_at":null,"retried_in_job_uuid":null,"retry_pending":false,
                    "passed":true,"soft_failed":false,"stack_errored":false,"will_fail":false,"exit_status":0,"state":"finished"}]})
            }
            PublicReadKind::JobArtifacts => {
                state.record(format!("buildkite-artifacts:{workflow}"));
                json!([])
            }
            kind => panic!("unexpected fixture Buildkite body: {kind:?}"),
        };
        Ok(serde_json::to_vec(&body).unwrap())
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    options: Options,
    project: CurrentProject,
    state: Rc<RefCell<State>>,
    index: Vec<u8>,
    refs: String,
}
impl Fixture {
    fn new(scenario: Scenario, cancel: &Cancellation) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".decapod")).unwrap();
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        command(&root, &["init", "-b", "work"]);
        command(&root, &["config", "user.name", "Automatic Runtime Fixture"]);
        command(&root, &["config", "user.email", "fixture@example.invalid"]);
        fs::write(root.join(CONFIG), "[proof]\n").unwrap();
        fs::write(root.join(GOVERNANCE), "{}\n").unwrap();
        fs::write(root.join("source"), "base").unwrap();
        command(&root, &["add", "."]);
        command(&root, &["commit", "-m", "base"]);
        let base = command(&root, &["rev-parse", "HEAD"]);
        command(&root, &["update-ref", "refs/remotes/origin/master", &base]);
        fs::write(root.join("source"), "candidate").unwrap();
        command(&root, &["add", "source"]);
        command(&root, &["commit", "-m", "candidate"]);
        let project = project(&root);
        let plan = source_import::source_run_plan(&project).unwrap();
        let state = Rc::new(RefCell::new(State {
            scenario,
            cancel: cancel.clone(),
            source_head: command(&root, &["rev-parse", "HEAD"]),
            source_tree: command(&root, &["rev-parse", "HEAD^{tree}"]),
            base,
            published: None,
            github: plan.github_workflows,
            buildkite: plan.buildkite_pipelines[0].workflows.clone(),
            cycle: 0,
            calls: BTreeMap::new(),
            events: vec![],
        }));
        let options = Options {
            root: root.canonicalize().unwrap(),
            git: PathBuf::from("/usr/bin/git").canonicalize().unwrap(),
            repository: "fixture/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: external,
            timeout: Duration::from_secs(30),
            max_input_bytes: 128 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
        };
        Self {
            _temp: temp,
            options,
            project,
            state,
            index: fs::read(root.join(".git/index")).unwrap(),
            refs: command(
                &root,
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            ),
        }
    }
    fn host(&self) -> SnapshotHost<'static, FixtureRunner> {
        SnapshotHost::new(
            self.options.clone(),
            Repository::new("fixture", "decapod").unwrap(),
            FixtureRunner {
                official: OfficialCli::git_only(self.options.git.clone(), |_, _| Ok(())).unwrap(),
                root: self.options.root.clone(),
                state: self.state.clone(),
            },
            SnapshotMode::PublishAndCollect,
            None,
            None,
            |_| Ok(ProjectPrerequisites::Passed),
        )
        .unwrap()
        .with_public_buildkite_reader(
            FixtureBuildkite(self.state.clone()),
            ReviewedBuildkiteImporter::reviewed(),
        )
        .unwrap()
    }
    fn assert_source_preserved(&self) {
        assert_eq!(
            command(&self.options.root, &["rev-parse", "HEAD"]),
            self.state.borrow().source_head
        );
        assert_eq!(
            fs::read(self.options.root.join(".git/index")).unwrap(),
            self.index
        );
        assert_eq!(
            command(
                &self.options.root,
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            ),
            self.refs
        );
        assert_eq!(command(&self.options.root, &["status", "--porcelain"]), "");
    }
    fn assert_no_custody(&self) {
        let state = self.state.borrow();
        assert_eq!(state.count("publish"), 1);
        assert_eq!(state.count("snapshot-exact-readback"), 1);
        assert!(
            state
                .events
                .iter()
                .all(|event| !event.starts_with("github-jobs:")
                    && !event.starts_with("github-artifacts:")
                    && !event.starts_with("github-zip:")
                    && !event.starts_with("buildkite-steps:")
                    && !event.starts_with("buildkite-jobs:")
                    && !event.starts_with("buildkite-artifacts:"))
        );
    }
}
fn runtime_budget(cancel: &Cancellation, timeout: Duration) -> RuntimeBudget<'_> {
    RuntimeBudget {
        deadline: Instant::now() + timeout,
        cancel,
        max_output_bytes: 1024 * 1024,
        bytes_left: 128 * 1024 * 1024,
    }
}

#[test]
fn automatic_publish_waits_for_queued_running_terminal_then_collects_once_and_stays_not_ready() {
    let cancel = Cancellation::default();
    let fixture = Fixture::new(Scenario::Progress, &cancel);
    let mut host = fixture.host();
    let result = host.select(
        &fixture.project,
        &runtime_budget(&cancel, Duration::from_secs(30)),
    );
    let blockers = result
        .err()
        .expect("aggregate provider success without custody is never readiness");
    assert!(
        blockers
            .iter()
            .any(|b| b.code == "SOURCE_EXECUTION_CUSTODY_BLOCKED"
                && b.detail.contains("required uploaded archive absent"))
    );
    assert!(
        blockers
            .iter()
            .any(|b| b.code == "BUILDKITE_SOURCE_CUSTODY_BLOCKED")
    );
    assert!(host.verified_checks.is_empty());
    assert!(
        host.terminal_sources.is_some(),
        "the exact terminal set must be rechecked after collection"
    );
    assert!(host.prepared_snapshot.is_some());
    let state = fixture.state.borrow();
    assert_eq!(state.count("publish"), 1);
    assert_eq!(state.count("snapshot-exact-readback"), 1);
    let first_custody = state
        .events
        .iter()
        .position(|e| e.starts_with("github-jobs:"))
        .unwrap();
    for status in [
        "github-state:queued",
        "github-state:in_progress",
        "github-state:completed",
        "buildkite-state:scheduled",
        "buildkite-state:running",
        "buildkite-state:passed",
    ] {
        assert!(
            state.events[..first_custody].iter().any(|e| e == status),
            "missing pre-custody transition {status}"
        );
    }
    // Each custody collector makes one initial inventory and one immutability
    // recheck. Repeated pending polls must not fetch jobs/artifacts/receipts.
    for workflow in &state.github {
        assert_eq!(state.count(&format!("github-jobs:{workflow}")), 2);
        assert_eq!(state.count(&format!("github-artifacts:{workflow}")), 2);
    }
    for workflow in &state.buildkite {
        assert_eq!(state.count(&format!("buildkite-steps:{workflow}")), 2);
        assert_eq!(state.count(&format!("buildkite-jobs:{workflow}")), 2);
        assert_eq!(state.count(&format!("buildkite-artifacts:{workflow}")), 1);
    }
    assert_eq!(state.count("github-zip:700"), 0);
    drop(state);
    let directory = host.evidence_directory().unwrap();
    assert!(directory.join("snapshot-handoff.json").is_file());
    assert!(directory.join("cleanup-instructions.txt").is_file());
    assert!(!fs::read_dir(directory).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("source-")
    }));
    fixture.assert_source_preserved();
}

#[test]
fn automatic_normal_path_downloads_selected_archive_once_and_rejects_invalid_custody() {
    let cancel = Cancellation::default();
    let fixture = Fixture::new(Scenario::CorruptArchive, &cancel);
    let mut host = fixture.host();
    assert!(
        host.artifact_directory.is_none(),
        "exercise normal configured download path"
    );
    let blockers = host
        .select(
            &fixture.project,
            &runtime_budget(&cancel, Duration::from_secs(20)),
        )
        .err()
        .unwrap();
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].code, "SNAPSHOT_NOT_READY");
    assert!(
        blockers[0]
            .detail
            .contains("GitHub artifact 700 for run 1000 attempt 1")
    );
    assert!(blockers[0].detail.contains("ZIP end record missing"));
    let state = fixture.state.borrow();
    assert_eq!(state.count("github-zip:700"), 1);
    assert!(
        !state
            .events
            .iter()
            .any(|e| e.starts_with("buildkite-steps:"))
    );
    drop(state);
    assert!(host.verified_checks.is_empty());
    assert!(host.terminal_sources.is_none());
    fixture.assert_source_preserved();
}

#[test]
fn automatic_provider_failure_or_cancellation_stops_before_custody() {
    for scenario in [
        Scenario::GithubTerminal("failure"),
        Scenario::GithubTerminal("cancelled"),
        Scenario::BuildkiteTerminal("failed"),
        Scenario::BuildkiteTerminal("canceled"),
    ] {
        let cancel = Cancellation::default();
        let fixture = Fixture::new(scenario, &cancel);
        let mut host = fixture.host();
        let error = host
            .stage(
                &fixture.project,
                &runtime_budget(&cancel, Duration::from_secs(20)),
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("completed without success"),
            "{scenario:?}: {error}"
        );
        assert!(host.terminal_sources.is_none());
        assert!(host.verified_checks.is_empty());
        fixture.assert_no_custody();
        fixture.assert_source_preserved();
    }
}

#[test]
fn automatic_missing_workflow_or_build_exhausts_shared_deadline_without_custody() {
    for scenario in [Scenario::MissingGithub, Scenario::MissingBuildkite] {
        let cancel = Cancellation::default();
        let fixture = Fixture::new(scenario, &cancel);
        let mut host = fixture.host();
        let error = host
            .stage(
                &fixture.project,
                &runtime_budget(&cancel, Duration::from_secs(10)),
            )
            .unwrap_err();
        assert_eq!(
            error.kind(),
            io::ErrorKind::TimedOut,
            "{scenario:?}: {error}"
        );
        assert!(host.terminal_sources.is_none());
        fixture.assert_no_custody();
        fixture.assert_source_preserved();
    }
}

#[test]
fn automatic_authorization_denial_and_local_cancellation_are_not_retried() {
    for (scenario, kind) in [
        (Scenario::DeniedMetadata, io::ErrorKind::PermissionDenied),
        (Scenario::CancelMetadata, io::ErrorKind::Interrupted),
    ] {
        let cancel = Cancellation::default();
        let fixture = Fixture::new(scenario, &cancel);
        let mut host = fixture.host();
        let error = host
            .stage(
                &fixture.project,
                &runtime_budget(&cancel, Duration::from_secs(20)),
            )
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        let state = fixture.state.borrow();
        assert_eq!(state.count(&format!("github-run:{}", state.github[0])), 1);
        assert!(
            !state
                .events
                .iter()
                .any(|e| e.starts_with("buildkite-state:"))
        );
        drop(state);
        assert!(host.terminal_sources.is_none());
        fixture.assert_no_custody();
        fixture.assert_source_preserved();
    }
}
