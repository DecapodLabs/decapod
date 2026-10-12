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

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Progress,
    BuildkiteTerminal(&'static str),
    MissingBuildkite,
    DeniedMetadata,
    CancelMetadata,
}
struct State {
    scenario: Scenario,
    cancel: Cancellation,
    source_head: String,
    source_tree: String,
    base: String,
    published: Option<(String, String)>,
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
    fn buildkite_status(&self) -> &'static str {
        match self.scenario {
            Scenario::Progress if self.cycle == 1 => "scheduled",
            Scenario::Progress if self.cycle == 2 => "running",
            Scenario::BuildkiteTerminal(state) => state,
            _ => "passed",
        }
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
        if spec.program() == Program::GitLocal {
            return self.official.execute(spec, budget);
        }
        if spec.program() == Program::GitTransport
            && spec.arguments().iter().any(|arg| arg == "config")
        {
            // The provider is synthetic; inspect only its actual fixture config.
            return provider::fixture_config::execute(&mut self.official, spec, budget);
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
            ProviderRequest::GithubGet { path } => {
                assert!(
                    !path.contains("/actions/"),
                    "Buildkite-only preflight must not read GitHub Actions execution APIs"
                );
                self.state
                    .borrow_mut()
                    .record(format!("github-read:{path}"));
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
            let (snapshot, _) = state.snapshot();
            assert!(endpoint.starts_with(&format!("commits/{snapshot}/statuses?")));
            state.record("buildkite-locators".into());
            if matches!(state.scenario, Scenario::DeniedMetadata) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture locator read denied",
                ));
            }
            if matches!(state.scenario, Scenario::CancelMetadata) {
                state.cancel.cancel();
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "fixture cancellation",
                ));
            }
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
                // Native envelopes advance independently of the cached GitHub
                // locator registry. Build 1 starts each complete metadata poll.
                if number == 1 {
                    state.cycle += 1;
                }
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
        fs::write(
            root.join(CONFIG),
            "[proof]\nvalidation_buildkite_preflights=true\n",
        )
        .unwrap();
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
        assert!(
            plan.github_workflows.is_empty(),
            "source preflight plan must be Buildkite-only"
        );
        let state = Rc::new(RefCell::new(State {
            scenario,
            cancel: cancel.clone(),
            source_head: command(&root, &["rev-parse", "HEAD"]),
            source_tree: command(&root, &["rev-parse", "HEAD^{tree}"]),
            base,
            published: None,
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
        let state = self.state.clone();
        let mut host = SnapshotHost::new(
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
            move |_| {
                state
                    .borrow_mut()
                    .record("provider-prerequisites-required".into());
                Ok(ProjectPrerequisites::ProviderRequired)
            },
        )
        .unwrap()
        .with_public_buildkite_reader(
            FixtureBuildkite(self.state.clone()),
            ReviewedBuildkiteImporter::reviewed(),
        )
        .unwrap();
        let cancel = self.state.borrow().cancel.clone();
        assert_eq!(
            host.project_check(&runtime_budget(&cancel, Duration::from_secs(30)))
                .unwrap(),
            ProjectPrerequisites::ProviderRequired,
        );
        assert!(self.state.borrow().published.is_none());
        host
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
        assert_eq!(state.count("provider-prerequisites-required"), 1);
        assert_eq!(state.count("publish"), 1);
        assert_eq!(state.count("snapshot-exact-readback"), 1);
        assert!(
            state
                .events
                .iter()
                .all(|event| !event.starts_with("buildkite-steps:")
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
fn automatic_buildkite_publish_waits_for_terminal_then_collects_once_and_stays_not_ready() {
    let cancel = Cancellation::default();
    let fixture = Fixture::new(Scenario::Progress, &cancel);
    let mut host = fixture.host();
    let governance = fs::read(fixture.options.root.join(GOVERNANCE)).unwrap();
    // Exercise the real controller with provider-owned prerequisites. No local
    // project suite or receipt exists; queued/running/terminal metadata and
    // source custody still determine its outcome.
    let blockers = host
        .select(
            &fixture.project,
            &runtime_budget(&cancel, Duration::from_secs(30)),
        )
        .err()
        .expect("aggregate provider success without source custody is never readiness");
    assert!(
        blockers
            .iter()
            .any(|b| b.code == "BUILDKITE_SOURCE_CUSTODY_BLOCKED"),
        "unexpected blockers: {blockers:#?}; provider events: {:#?}",
        fixture.state.borrow().events
    );
    assert_eq!(
        fs::read(fixture.options.root.join(GOVERNANCE)).unwrap(),
        governance
    );
    assert!(host.verified_checks.is_empty());
    assert!(host.terminal_sources.is_some());
    assert!(host.prepared_snapshot.is_some());
    let state = fixture.state.borrow();
    assert_eq!(state.count("provider-prerequisites-required"), 1);
    assert_eq!(
        state.events.first().map(String::as_str),
        Some("provider-prerequisites-required")
    );
    assert_eq!(state.count("publish"), 1);
    assert_eq!(state.count("snapshot-exact-readback"), 1);
    let first_custody = state
        .events
        .iter()
        .position(|e| e.starts_with("buildkite-steps:"))
        .unwrap();
    for status in [
        "buildkite-state:scheduled",
        "buildkite-state:running",
        "buildkite-state:passed",
    ] {
        assert!(
            state.events[..first_custody]
                .iter()
                .any(|event| event == status),
            "missing pre-custody transition {status}"
        );
    }
    let scheduled = state
        .events
        .iter()
        .position(|event| event == "buildkite-state:scheduled")
        .unwrap();
    let running = state
        .events
        .iter()
        .position(|event| event == "buildkite-state:running")
        .unwrap();
    let passed = state
        .events
        .iter()
        .position(|event| event == "buildkite-state:passed")
        .unwrap();
    assert!(scheduled < running && running < passed && passed < first_custody);
    assert_eq!(
        state.events[..passed]
            .iter()
            .filter(|event| event.as_str() == "buildkite-locators")
            .count(),
        1,
        "native progress must not require repeated locator discovery"
    );
    // One collection plus its immutable inventory recheck. Pending metadata
    // polls must never fetch jobs, steps, artifacts or execution receipts.
    for workflow in &state.buildkite {
        assert_eq!(state.count(&format!("buildkite-steps:{workflow}")), 2);
        assert_eq!(state.count(&format!("buildkite-jobs:{workflow}")), 2);
        assert_eq!(state.count(&format!("buildkite-artifacts:{workflow}")), 1);
    }
    assert!(
        state
            .events
            .iter()
            .filter(|e| e.starts_with("github-read:"))
            .all(|e| !e.contains("/actions/"))
    );
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
fn automatic_buildkite_native_green_without_actual_setup_never_supplies_candidate() {
    let cancel = Cancellation::default();
    let fixture = Fixture::new(Scenario::BuildkiteTerminal("passed"), &cancel);
    let candidate = fixture
        .project
        .catalog
        .obligations()
        .iter()
        .find(|obligation| obligation.kind == evidence::CheckKind::CandidateBuild)
        .expect("complete catalog retains actual candidate build");
    let setup = fixture
        .project
        .catalog
        .obligations()
        .iter()
        .find(|obligation| {
            obligation.kind == evidence::CheckKind::SourceCi
                && obligation.key.provider == evidence::Provider::Buildkite
                && obligation.key.profile.ends_with("/setup-bazel")
        })
        .expect("candidate source build remains a required actual Buildkite setup check");
    assert!(
        fixture
            .project
            .catalog
            .obligations()
            .iter()
            .all(
                |obligation| obligation.kind != evidence::CheckKind::SourceCi
                    || obligation.key.provider == evidence::Provider::Buildkite
            )
    );
    let mut host = fixture.host();
    let blockers = host
        .select(
            &fixture.project,
            &runtime_budget(&cancel, Duration::from_secs(20)),
        )
        .err()
        .unwrap();
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker.check.as_ref() == Some(&candidate.key))
    );
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker.check.as_ref() == Some(&setup.key)
                && blocker.code == "BUILDKITE_SOURCE_CUSTODY_BLOCKED")
    );
    assert!(!host.verified_checks.contains_key(&candidate.key));
    assert!(!host.verified_checks.contains_key(&setup.key));
    assert!(host.verified_checks.is_empty());
    fixture.assert_source_preserved();
}

#[test]
fn automatic_buildkite_failure_or_cancellation_stops_before_custody() {
    for scenario in [
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
fn automatic_missing_buildkite_build_exhausts_shared_deadline_without_custody() {
    let cancel = Cancellation::default();
    let fixture = Fixture::new(Scenario::MissingBuildkite, &cancel);
    let mut host = fixture.host();
    let error = host
        .stage(
            &fixture.project,
            &runtime_budget(&cancel, Duration::from_secs(10)),
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(host.terminal_sources.is_none());
    fixture.assert_no_custody();
    fixture.assert_source_preserved();
}

#[test]
fn automatic_buildkite_locator_denial_and_local_cancellation_are_not_retried() {
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
        assert_eq!(state.count("buildkite-locators"), 1);
        assert!(
            !state
                .events
                .iter()
                .any(|event| event.starts_with("buildkite-state:"))
        );
        drop(state);
        assert!(host.terminal_sources.is_none());
        fixture.assert_no_custody();
        fixture.assert_source_preserved();
    }
}
