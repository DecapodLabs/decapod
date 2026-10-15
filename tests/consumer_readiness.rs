//! Consumer readiness is exercised through the public runtime and its real
//! repository/native adapters. Only the accepted-target transport is replaced
//! with an independently read local Git upstream; no execution receipt is faked.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::native_proof::ReviewedCapabilities;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_evidence::{
    Conclusion, ExpectedPlan, MaterialIdentity, Observation, Provider, SuiteEvidence,
};
use decapod::core::readiness_profile::{self, ProfileDeclaration, ProjectPrerequisites};
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime::{
    self as runtime, Blocker, CurrentProject, ExecutedSuite, ExecutionSelection, Host,
    NativeAction, Options, RuntimeBudget, ValidationReport,
};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const CONFIG: &str = ".decapod/config.toml";
const WORKFLOW: &str = ".github/workflows/decapod-validate.yml";

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Consumer Readiness Test")
        .env("GIT_COMMITTER_NAME", "Consumer Readiness Test")
        .env("GIT_AUTHOR_EMAIL", "consumer@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "consumer@example.invalid")
        .bounded_output(Duration::from_secs(10))
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn python_runtime() -> (String, BTreeMap<String, String>) {
    let program = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find_map(|path| Path::new(path).canonicalize().ok())
        .expect("consumer execution regression requires installed Python 3");
    // Measure real installed runtime files, not synthetic success executables.
    // The declaration is explicit; these tests make no inferred-closure claim.
    let output = Command::new(&program)
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            "import os, encodings, encodings.utf_8; print(os.path.realpath(os.__file__)); print(os.path.realpath(encodings.__file__)); print(os.path.realpath(encodings.utf_8.__file__))",
        ])
        .bounded_output(Duration::from_secs(10))
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let closure = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(index, path)| {
            assert!(
                Path::new(path).is_file(),
                "missing Python runtime file: {path}"
            );
            (format!("python-runtime-{index}"), path.to_owned())
        })
        .collect();
    (program.to_string_lossy().into_owned(), closure)
}

struct Fixture {
    _storage: tempfile::TempDir,
    upstream: PathBuf,
    options: Options,
    proof: ProjectProofConfig,
}

impl Fixture {
    fn new() -> Self {
        Self::with_accepted_config(None)
    }

    fn with_accepted_profile(id: &str) -> Self {
        Self::with_accepted_config(Some(&format!(
            "[proof.required_profile]\nid = {id:?}\nreviewed_sources_digest = {:?}\n",
            format!("sha256:{}", "0".repeat(64)),
        )))
    }

    fn with_accepted_config(accepted_config: Option<&str>) -> Self {
        let storage = tempfile::tempdir().unwrap();
        let root = storage.path().join("consumer");
        let upstream = storage.path().join("upstream.git");
        let evidence = storage.path().join("evidence");
        fs::create_dir_all(root.join(".decapod")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir(root.join("scripts")).unwrap();
        fs::create_dir(&evidence).unwrap();
        fs::create_dir(&upstream).unwrap();
        fs::write(root.join("payload.txt"), "ready\n").unwrap();
        fs::write(root.join("second.txt"), "also ready\n").unwrap();
        fs::write(
            root.join("Justfile"),
            "check:\n    python3 -B scripts/check_source.py\n",
        )
        .unwrap();
        fs::write(
            root.join(WORKFLOW),
            "name: Governed consumer\non: [pull_request]\njobs:\n  validate:\n    runs-on: ubuntu-latest\n    steps:\n      - run: decapod validate\n",
        )
        .unwrap();
        fs::write(
            root.join("scripts/check_source.py"),
            "import os\nassert not os.path.exists('Cargo.toml')\nassert open('payload.txt').read() == 'ready\\n', 'consumer source is invalid'\nassert os.getcwd() != os.environ['FIXTURE_WORKTREE'], 'proof used mutable worktree'\nassert os.environ['DECAPOD_NATIVE_PROOF_ACTIVE'] == '1'\nassert os.path.isdir(os.environ['HOME'])\nprint('consumer-source-inspected')\n",
        )
        .unwrap();
        fs::write(root.join(".decapod/governance.json"), "{\"sections\":{}}\n").unwrap();
        // Model first adoption: the accepted target can predate Decapod config.
        if let Some(config) = accepted_config {
            fs::write(root.join(CONFIG), config).unwrap();
        }
        git(&root, &["init", "-b", "main"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "accepted consumer source"]);
        git(&upstream, &["init", "--bare"]);
        git(
            &root,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );
        git(&root, &["push", "origin", "main"]);
        git(&root, &["checkout", "-b", "consumer-work"]);
        let options = Options {
            root: root.canonicalize().unwrap(),
            git: Path::new("/usr/bin/git").canonicalize().unwrap(),
            repository: "consumer/project".into(),
            base_branch: "main".into(),
            accepted_base_ref: "refs/remotes/origin/main".into(),
            evidence_parent: evidence,
            timeout: Duration::from_secs(60),
            max_input_bytes: 64 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
        };
        let (program, toolchain_files) = python_runtime();
        let mut commands = vec![
            ProofDef {
                name: "inspect-source".into(),
                command: program.clone(),
                args: ["-I", "-S", "-B", "scripts/check_source.py"]
                    .map(str::to_owned).into(),
                description: "Read actual committed consumer source".into(),
                required: true,
            },
            ProofDef {
                name: "inspect-second".into(),
                command: program.clone(),
                args: ["-I", "-S", "-B", "-c", "assert open('second.txt').read() == 'also ready\\n'; print('second-source-inspected')"]
                    .map(str::to_owned).into(),
                description: "A second independent required source assertion".into(),
                required: true,
            },
        ];
        // Public project checks require canonical proof-name ordering too.
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        let execution = RepositoryExecution {
            program,
            toolchain_files,
            environment: BTreeMap::from([(
                "FIXTURE_WORKTREE".into(),
                options.root.to_string_lossy().into_owned(),
            )]),
            directory: ".".into(),
            timeout_seconds: 10,
        };
        let mut contract = RepositoryContract {
            reviewed_commands_digest: String::new(),
            source_coverage: vec![SourceCoverage {
                source: WORKFLOW.into(),
                commands: commands
                    .iter()
                    .map(|command| command.name.clone())
                    .collect(),
            }],
            executions: commands
                .iter()
                .map(|command| (command.name.clone(), execution.clone()))
                .collect(),
        };
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&commands, &contract).unwrap();
        let sources =
            runtime::discover_source_inventory(&options, &Cancellation::default()).unwrap();
        let proof = ProjectProofConfig {
            validation_buildkite_preflights: false,
            required_profile: Some(ProfileDeclaration {
                id: repository_profile::REQUIRED_PROFILE.into(),
                reviewed_sources_digest: sources.digest().unwrap(),
            }),
            repository_contract: Some(contract),
            ci_attested: None,
            commands,
        };
        let fixture = Self {
            _storage: storage,
            upstream,
            options,
            proof,
        };
        fixture.save_config();
        fixture.commit("declare reviewed local consumer contract");
        fixture
    }

    fn save_config(&self) {
        let config = toml::Value::try_from(serde_json::json!({"proof": self.proof})).unwrap();
        fs::write(
            self.options.root.join(CONFIG),
            toml::to_string_pretty(&config).unwrap(),
        )
        .unwrap();
    }

    fn review_commands(&mut self) {
        let contract = self.proof.repository_contract.as_mut().unwrap();
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&self.proof.commands, contract).unwrap();
    }

    fn commit(&self, message: &str) {
        git(&self.options.root, &["add", "."]);
        git(&self.options.root, &["commit", "-m", message]);
    }

    fn host(&self) -> LocalHost {
        LocalHost::new(self, false)
    }

    fn validate(&self, host: &mut LocalHost) -> ValidationReport {
        runtime::validate(&self.options, host, &Cancellation::default())
    }
}

struct LocalHost {
    upstream: PathBuf,
    root: PathBuf,
    runner: runtime::repository_host::RepositoryHost<'static>,
    selections: usize,
    execution_attempts: usize,
    change_source_after_selection: bool,
    move_target_after_execution: bool,
    change_config_after_execution: bool,
    force_strict_capability: bool,
}

impl LocalHost {
    fn new(fixture: &Fixture, deny_execution: bool) -> Self {
        let store = fixture.options.root.join(".decapod/data");
        let runner = runtime::repository_host::RepositoryHost::new(
            fixture.options.clone(),
            "https://github.com/consumer/project.git".into(),
            |_| Ok(ProjectPrerequisites::Passed),
            move |action| {
                let NativeAction::RepositoryCheck { invocation, seal } = action else {
                    panic!("repository execution must use the sealed authorization path");
                };
                if deny_execution {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "fixture denied proof execution",
                    ))
                } else {
                    decapod::core::external_action::authorize_repository_proof(
                        &store, invocation, seal,
                    )
                    .map_err(io::Error::other)
                }
            },
        )
        .unwrap();
        Self {
            upstream: fixture.upstream.clone(),
            root: fixture.options.root.clone(),
            runner,
            selections: 0,
            execution_attempts: 0,
            change_source_after_selection: false,
            move_target_after_execution: false,
            change_config_after_execution: false,
            force_strict_capability: false,
        }
    }
}

impl Host for LocalHost {
    fn project_check(&mut self, _: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> {
        Ok(ProjectPrerequisites::Passed)
    }

    fn accepted_target(
        &mut self,
        repository: &str,
        branch: &str,
        _: &RuntimeBudget<'_>,
    ) -> io::Result<String> {
        assert_eq!(repository, "consumer/project");
        assert_eq!(branch, "main");
        // Read the upstream repository afresh, not the candidate tracking ref.
        Ok(git(&self.upstream, &["rev-parse", "refs/heads/main"]))
    }

    fn select(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>> {
        self.selections += 1;
        let mut selection = self.runner.select(project, budget)?;
        if self.force_strict_capability {
            for (key, invocation) in &mut selection.native_invocations {
                invocation.capabilities = ReviewedCapabilities::LocalCheckWithOwnedScratch;
                selection.bindings.get_mut(key).unwrap().invocation_digest =
                    invocation.digest().unwrap();
            }
        }
        if self.change_source_after_selection {
            fs::write(self.root.join("payload.txt"), "changed during selection\n").unwrap();
        }
        Ok(selection)
    }

    fn execute(
        &mut self,
        project: &CurrentProject,
        selection: &ExecutionSelection,
        plan: &ExpectedPlan,
        identity: &MaterialIdentity,
        directory: &Path,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite> {
        self.execution_attempts += 1;
        // This one negative case targets the legacy runner capability directly;
        // production host seals correctly reject invocation mutation even earlier.
        let result = if self.force_strict_capability {
            runtime::execute_native(project, selection, plan, identity, directory, budget)?
        } else {
            self.runner
                .execute(project, selection, plan, identity, directory, budget)?
        };
        if self.move_target_after_execution {
            git(&self.root, &["push", "origin", "HEAD:refs/heads/main"]);
        }
        if self.change_config_after_execution {
            let path = self.root.join(CONFIG);
            let mut bytes = fs::read(&path)?;
            bytes.extend_from_slice(b"\n# configuration changed after successful execution\n");
            fs::write(path, bytes)?;
        }
        Ok(result)
    }
}

fn blocked(report: &ValidationReport, expected: &str) {
    assert!(!report.ready, "unexpected readiness: {report:?}");
    assert_eq!(report.exit_code(), 1);
    assert!(
        report
            .blockers
            .iter()
            .any(|blocker| blocker.detail.contains(expected) || blocker.code.contains(expected)),
        "expected {expected}: {report:?}"
    );
}

#[test]
fn fresh_non_cargo_consumer_executes_every_required_source_check_in_snapshot() {
    let fixture = Fixture::new();
    assert!(!fixture.options.root.join("Cargo.toml").exists());
    let head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
    let upstream = git(&fixture.upstream, &["rev-parse", "refs/heads/main"]);
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 0);
    assert_eq!(report.scope, "repository_local_readiness");
    assert_eq!(host.execution_attempts, 1);
    let directory = report.evidence_path.unwrap();
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    assert_eq!(evidence.results.len(), 2);
    for result in &evidence.results {
        assert_eq!(result.executor, Provider::Native);
        assert_eq!(result.key.profile, repository_profile::REQUIRED_PROFILE);
        assert!(matches!(
            result.observation,
            Observation::Executed {
                conclusion: Conclusion::Success,
                exit_code: Some(0),
                ..
            }
        ));
    }
    let stdout = [0, 1]
        .map(|index| fs::read_to_string(directory.join(format!("native-{index}.stdout"))).unwrap())
        .join("\n");
    assert!(stdout.contains("consumer-source-inspected"));
    assert!(stdout.contains("second-source-inspected"));
    assert!(
        report
            .stage_dispositions
            .iter()
            .any(|stage| stage.evidence_scope == "no_hosted_ci_or_release_assurance")
    );
    assert_eq!(git(&fixture.options.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&fixture.upstream, &["rev-parse", "refs/heads/main"]),
        upstream
    );
    assert!(git(&fixture.options.root, &["status", "--porcelain"]).is_empty());
}

#[test]
fn real_source_failure_is_not_ready_and_committed_repair_runs_new_proof() {
    let fixture = Fixture::new();
    fs::write(fixture.options.root.join("payload.txt"), "broken\n").unwrap();
    fixture.commit("introduce source defect");
    let mut host = fixture.host();
    let failed = fixture.validate(&mut host);
    assert!(!failed.ready, "{failed:?}");
    assert_eq!(
        host.execution_attempts, 1,
        "failure must come from real execution"
    );
    let failed_path = failed.evidence_path.as_ref().unwrap();
    let stderr = fs::read_to_string(failed_path.join("native-1.stderr")).unwrap_or_default()
        + &fs::read_to_string(failed_path.join("native-0.stderr")).unwrap_or_default();
    assert!(
        stderr.contains("consumer source is invalid"),
        "{failed:?}: {stderr}"
    );
    fs::write(fixture.options.root.join("payload.txt"), "ready\n").unwrap();
    fixture.commit("repair actual source defect");
    let repaired = fixture.validate(&mut fixture.host());
    assert!(repaired.ready, "{repaired:?}");
    assert_ne!(failed.evidence_path, repaired.evidence_path);
}

#[test]
fn unknown_profile_is_rejected_before_adapter_selection() {
    let mut fixture = Fixture::new();
    fixture.proof.required_profile.as_mut().unwrap().id = "repo.pr-readiness.v999".into();
    fixture.save_config();
    fixture.commit("unsupported profile");
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "UNKNOWN_PROOF_PROFILE");
    assert_eq!(host.selections, 0);
}

#[test]
fn changed_workflow_and_uppercase_justfile_require_source_review() {
    for path in [WORKFLOW, "Justfile"] {
        let fixture = Fixture::new();
        let sources =
            runtime::discover_source_inventory(&fixture.options, &Cancellation::default()).unwrap();
        assert!(
            sources.files.contains_key(path),
            "recipe omitted from discovery: {path}"
        );
        let mut bytes = fs::read(fixture.options.root.join(path)).unwrap();
        bytes.extend_from_slice(b"\n# newly required source check\n");
        fs::write(fixture.options.root.join(path), bytes).unwrap();
        fixture.commit("change reviewed build definition");
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "PROOF_PROFILE_RECONCILIATION_REQUIRED",
        );
        assert_eq!(host.selections, 0);
    }
}

#[test]
fn changed_command_and_execution_fields_require_explicit_review() {
    for field in [
        "args",
        "program",
        "environment",
        "toolchain",
        "directory",
        "timeout",
        "required",
    ] {
        let mut fixture = Fixture::new();
        let execution = fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .executions
            .get_mut("inspect-source")
            .unwrap();
        match field {
            "args" => fixture.proof.commands[0].args.push("unreviewed".into()),
            "program" => execution.program = "/unreviewed/python3".into(),
            "environment" => {
                execution
                    .environment
                    .insert("NEW_INPUT".into(), "changed".into());
            }
            "toolchain" => {
                execution
                    .toolchain_files
                    .insert("new-runtime".into(), "/unreviewed/runtime".into());
            }
            "directory" => execution.directory = "scripts".into(),
            "timeout" => execution.timeout_seconds += 1,
            "required" => fixture.proof.commands[0].required = false,
            _ => unreachable!(),
        }
        fixture.save_config();
        fixture.commit("change proof contract without review");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "STALE_COMMAND_CONTRACT");
        assert_eq!(host.selections, 0, "{field} escaped review binding");
    }
}

#[test]
fn reviewed_but_missing_duplicate_or_unknown_coverage_fails_closed() {
    for defect in [
        "missing-workflow",
        "duplicate-workflow",
        "duplicate-command",
        "unknown-command",
        "unmapped-command",
        "unknown-source",
    ] {
        let mut fixture = Fixture::new();
        let coverage = &mut fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .source_coverage;
        match defect {
            "missing-workflow" => coverage.clear(),
            "duplicate-workflow" => coverage.push(coverage[0].clone()),
            "duplicate-command" => coverage[0].commands.push("inspect-source".into()),
            "unknown-command" => coverage[0].commands.push("not-a-proof".into()),
            "unmapped-command" => {
                coverage[0].commands.pop();
            }
            "unknown-source" => coverage[0].source = ".github/workflows/absent.yml".into(),
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("review structurally incomplete coverage");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "REPOSITORY_CONTRACT_INVALID");
        assert_eq!(host.selections, 0, "{defect} admitted");
    }
}

#[test]
fn reviewed_empty_optional_and_duplicate_required_commands_are_not_admitted() {
    for defect in ["empty", "optional", "duplicate", "missing-execution"] {
        let mut fixture = Fixture::new();
        match defect {
            "empty" => fixture.proof.commands.clear(),
            "optional" => fixture.proof.commands[0].required = false,
            "duplicate" => fixture
                .proof
                .commands
                .push(fixture.proof.commands[0].clone()),
            "missing-execution" => {
                fixture
                    .proof
                    .repository_contract
                    .as_mut()
                    .unwrap()
                    .executions
                    .remove("inspect-source");
            }
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("review invalid required proof set");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "REPOSITORY_CONTRACT_INVALID");
        assert_eq!(host.selections, 0, "{defect} admitted");
    }
}

#[test]
fn accepted_strict_profile_cannot_be_downgraded_by_candidate_local_contract() {
    let fixture = Fixture::with_accepted_profile(readiness_profile::REQUIRED_PROFILE);
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "REQUIRED_PROFILE_DOWNGRADE");
    assert_eq!(host.selections, 0);
}

#[test]
fn source_change_during_selection_prevents_any_execution() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.change_source_after_selection = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn accepted_target_moving_after_successful_checks_prevents_readiness() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.move_target_after_execution = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 1);
    assert!(
        report
            .evidence_path
            .unwrap()
            .join("observations.json")
            .is_file()
    );
}

#[test]
fn proof_authorization_denial_stops_before_native_execution() {
    let fixture = Fixture::new();
    let mut host = LocalHost::new(&fixture, true);
    let report = fixture.validate(&mut host);
    blocked(&report, "fixture denied proof execution");
    let directory = report.evidence_path.unwrap();
    assert!(!directory.join("observations.json").exists());
    assert!(!directory.join("native-0.stdout").exists());
}

#[test]
fn python_permission_does_not_relax_the_existing_strict_native_capability() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.force_strict_capability = true;
    blocked(&fixture.validate(&mut host), "interpreter");
}

#[test]
fn repository_local_profile_cannot_claim_buildkite_provider_execution() {
    let mut fixture = Fixture::new();
    fixture.proof.validation_buildkite_preflights = true;
    fixture.save_config();
    fixture.commit("request unsupported provider");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "REPOSITORY_PROFILE_PROVIDER_UNSUPPORTED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn public_dispatch_selects_repository_host_before_reading_a_cargo_manifest() {
    // This guards the public CLI/RPC routing seam separately from the real
    // runtime execution above; it is not represented as a CLI end-to-end run.
    let source = include_str!("../src/decapod/lib.rs");
    let body = source
        .split_once("fn run_current_workbranch_readiness_with(")
        .unwrap()
        .1
        .split_once("\nfn ")
        .unwrap()
        .0;
    let repository_host = body
        .find("runtime::repository_host::RepositoryHost::new(")
        .unwrap();
    let cargo_manifest = body.find("workspace_root.join(\"Cargo.toml\")").unwrap();
    assert!(repository_host < cargo_manifest);
    assert!(body[repository_host..cargo_manifest].contains("return Ok(report)"));
}

#[test]
fn malformed_accepted_configuration_is_not_treated_as_first_adoption() {
    let fixture = Fixture::with_accepted_config(Some("[proof.required_profile\nid = 'broken'\n"));
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert!(!report.blockers.is_empty());
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn configuration_change_after_successful_execution_invalidates_readiness() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.change_config_after_execution = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 1);
    let directory = report.evidence_path.unwrap();
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    assert_eq!(evidence.results.len(), 2);
    assert!(evidence.results.iter().all(|result| matches!(
        result.observation,
        Observation::Executed {
            conclusion: Conclusion::Success,
            exit_code: Some(0),
            ..
        }
    )));
}
