//! Consumer readiness is exercised through the public runtime and its real
//! repository/native adapters. Only the accepted-target transport is replaced
//! with an independently read local Git upstream; no execution receipt is faked.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::native_proof::ReviewedCapabilities;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_evidence::{ExpectedPlan, MaterialIdentity};
use decapod::core::readiness_profile::{ProfileDeclaration, ProjectPrerequisites};
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime::{
    self as runtime, Blocker, CurrentProject, ExecutedSuite, ExecutionSelection, Host,
    NativeAction, Options, RuntimeBudget, ValidationReport,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

pub(super) const CONFIG: &str = ".decapod/config.toml";
pub(super) const WORKFLOW: &str = ".github/workflows/decapod-validate.yml";

pub(super) fn git(root: &Path, args: &[&str]) -> String {
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

pub(super) struct Fixture {
    pub(super) _storage: tempfile::TempDir,
    pub(super) upstream: PathBuf,
    pub(super) options: Options,
    pub(super) proof: ProjectProofConfig,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self::with_accepted_config(None)
    }

    pub(super) fn with_accepted_config(accepted_config: Option<&str>) -> Self {
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
        // Use the production canonical constructor; terminal rules reject
        // unversioned governance stubs. Runtime locks remain outside Git.
        fs::write(
            root.join(".gitignore"),
            ".decapod/data/\n.decapod/managed/governance-document.lock\n",
        )
        .unwrap();
        decapod::core::governance_document::migrate(&root).unwrap();
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
                ci_only: false,
                reason: None,
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
            rust_workers: None,
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

    pub(super) fn save_config(&self) {
        let config = toml::Value::try_from(serde_json::json!({"proof": self.proof})).unwrap();
        fs::write(
            self.options.root.join(CONFIG),
            toml::to_string_pretty(&config).unwrap(),
        )
        .unwrap();
    }

    pub(super) fn review_commands(&mut self) {
        let contract = self.proof.repository_contract.as_mut().unwrap();
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&self.proof.commands, contract).unwrap();
    }

    pub(super) fn commit(&self, message: &str) {
        git(&self.options.root, &["add", "."]);
        git(&self.options.root, &["commit", "-m", message]);
    }

    pub(super) fn host(&self) -> LocalHost {
        LocalHost::new(self, false)
    }

    pub(super) fn validate(&self, host: &mut LocalHost) -> ValidationReport {
        runtime::validate(&self.options, host, &Cancellation::default())
    }
}

pub(super) struct GovernedAdmission {
    pub(super) storage: PathBuf,
    pub(super) observation: Rc<RefCell<Option<serde_json::Value>>>,
}

pub(super) struct LocalHost {
    upstream: PathBuf,
    root: PathBuf,
    runner: runtime::repository_host::RepositoryHost<'static>,
    pub(super) selections: usize,
    pub(super) execution_attempts: usize,
    pub(super) change_source_after_selection: bool,
    pub(super) move_target_after_execution: bool,
    pub(super) change_config_after_execution: bool,
    pub(super) force_strict_capability: bool,
    pub(super) governed_admission: Option<GovernedAdmission>,
}

impl LocalHost {
    pub(super) fn new(fixture: &Fixture, deny_execution: bool) -> Self {
        let store =
            decapod::core::store::find_governance_root(&fixture.options.root).join(".decapod/data");
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
            governed_admission: None,
        }
    }
}

impl Host for LocalHost {
    fn project_check(&mut self, budget: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> {
        if let Some(admission) = &self.governed_admission {
            let head = git(&self.root, &["rev-parse", "HEAD"]);
            // Ask the genuine CLI for explicitly partial admission. Its scope
            // cannot authorize readiness; the runtime below must execute proof.
            let output = governed_command(
                &self.root,
                &admission.storage,
                &["validate", "--format", "json"],
            )
            .env("GITHUB_ACTIONS", "true")
            .bounded_output(budget.remaining()?.min(Duration::from_secs(30)))?;
            let result: serde_json::Value =
                serde_json::from_slice(&output.stdout).map_err(|error| {
                    io::Error::other(format!(
                        "actual CLI admission JSON: {error}; stdout={} stderr={}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ))
                })?;
            if !output.status.success()
                || result["scope"] != "ci_local"
                || result["publication_ready"] != false
                || result.get("readiness") != Some(&serde_json::Value::Null)
                || result["report"]["fail_count"] != 0
                || result["report"]["pass_count"]
                    .as_u64()
                    .is_none_or(|count| count == 0)
            {
                return Err(io::Error::other(format!(
                    "actual CLI prerequisite admission failed: {result}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            if head != git(&self.root, &["rev-parse", "HEAD"])
                || !git(&self.root, &["status", "--porcelain"]).is_empty()
            {
                return Err(io::Error::other(
                    "actual CLI admission changed the committed candidate",
                ));
            }
            *admission.observation.borrow_mut() = Some(result);
        }
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

// Prepare a genuine docs task through the public CLI. Admission is explicitly
// CI-local; the complete public runtime independently executes source proof.
// Only its existing Host target transport reads a real local bare upstream.
#[cfg(unix)]
pub(super) fn governed_command(root: &Path, storage: &Path, args: &[&str]) -> Command {
    let binary = Path::new(
        option_env!("CARGO_BIN_EXE_decapod")
            .expect("CLI admission requires the actual integration-test binary"),
    )
    .canonicalize()
    .or_else(|_| {
        std::env::var_os("RUNFILES_DIR")
            .map(PathBuf::from)
            .map(|path| path.join("_main/decapod"))
            .ok_or_else(|| io::Error::other("missing Decapod test binary"))
    })
    .unwrap();
    let mut command = Command::new(binary);
    for (name, _) in std::env::vars_os() {
        let name_text = name.to_string_lossy();
        if name_text.starts_with("DECAPOD_")
            || name_text.starts_with("GIT_")
            || name_text.starts_with("BUILDKITE")
            || name_text.starts_with("GITHUB_")
            || name_text.starts_with("XDG_")
        {
            command.env_remove(name);
        }
    }
    command
        .args(args)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "docs-bootstrap")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("HOME", storage.join("home"))
        .env("XDG_CONFIG_HOME", storage.join("home/config"))
        .env("XDG_DATA_HOME", storage.join("home/data"))
        .env("XDG_CACHE_HOME", storage.join("home/cache"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("PATH", "/usr/bin:/bin:/usr/local/bin");
    command
}
