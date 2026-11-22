//! Actual-binary adversarial & repair regression suite for Decapod P0 #1660
//! ("Require observed kernel proof for VERIFIED workunit transitions").
//!
//! Exercises all 6 `#1660` falsifiers through the real compiled `decapod` CLI
//! binary without any `DECAPOD_VALIDATE_SKIP_*` test gate-skip variables:
//! (1) self-authored `record-proof` pass + `sha256:abc` cannot mint current
//!     `VERIFIED` or publication;
//! (2) real attempt with unavailable/skipped/zero-assertion output stays
//!     `not_proven`;
//! (3) old green receipt on stale subject/plan/accepted base or newer failure
//!     becomes `stale`;
//! (4) authentic proof copied from another task/repo/custody generation is
//!     rejected;
//! (5) split-store / workspace-local historical events are retained for audit
//!     (`not_proven`) without automatic `VERIFIED` promotion and return
//!     `WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`;
//! (6) repaired current task with genuine executed canonical proof completes
//!     `VERIFIED` atomically through the supported path.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::governance_document;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime as runtime;
use decapod::core::{workspace as ws, workunit};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const CONFIG: &str = ".decapod/config.toml";
const WORKFLOW: &str = ".github/workflows/decapod-validate.yml";

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    PathBuf::from(cargo_bin)
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Workunit Proof 1660 Test")
        .env("GIT_COMMITTER_NAME", "Workunit Proof 1660 Test")
        .env("GIT_AUTHOR_EMAIL", "wu1660@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "wu1660@example.invalid")
        .bounded_output(Duration::from_secs(20))
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
        .expect("Python 3 runtime is required");
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
        .map(|(index, path)| (format!("python-runtime-{index}"), path.to_owned()))
        .collect();
    (program.to_string_lossy().into_owned(), closure)
}

fn governed_command(root: &Path, storage: &Path, agent_id: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(resolve_decapod_bin());
    cmd.env_clear()
        .env("HOME", storage.join("home"))
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_STORAGE_OWNER", "1")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .current_dir(root)
        .args(args);
    cmd
}

fn governed_command_with_pass(
    root: &Path,
    storage: &Path,
    agent_id: &str,
    password: &str,
    args: &[&str],
) -> Command {
    let mut cmd = governed_command(root, storage, agent_id, args);
    cmd.env("DECAPOD_SESSION_PASSWORD", password);
    cmd
}

fn governed_ok(
    root: &Path,
    storage: &Path,
    agent_id: &str,
    password: &str,
    args: &[&str],
) -> serde_json::Value {
    let output = governed_command_with_pass(root, storage, agent_id, password, args)
        .bounded_output(Duration::from_secs(120))
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

fn governed_err(
    root: &Path,
    storage: &Path,
    agent_id: &str,
    password: &str,
    args: &[&str],
) -> String {
    let output = governed_command_with_pass(root, storage, agent_id, password, args)
        .bounded_output(Duration::from_secs(120))
        .unwrap();
    assert!(
        !output.status.success(),
        "{args:?} unexpectedly succeeded:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn acquire_session(root: &Path, storage: &Path, agent_id: &str) -> String {
    let out = governed_command(root, storage, agent_id, &["session", "acquire"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(|s| s.trim().to_owned()))
        .expect("session password")
}

struct GovernedRepoFixture {
    _storage: TempDir,
    storage: PathBuf,
    root: PathBuf,
    _upstream: PathBuf,
    evidence: PathBuf,
    proof: ProjectProofConfig,
    failure_flag: PathBuf,
}

impl GovernedRepoFixture {
    fn new(check_script: Option<&str>) -> Self {
        let storage_dir = tempfile::tempdir().unwrap();
        let storage = storage_dir.path().to_path_buf();
        let root = storage.join("repo");
        let upstream = storage.join("upstream.git");
        let evidence = storage.join("evidence");
        let home = storage.join("home");
        let failure_flag = storage.join("external-failure.flag");

        fs::create_dir_all(root.join(".decapod")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(&upstream).unwrap();
        fs::create_dir_all(&evidence).unwrap();
        fs::create_dir_all(&home).unwrap();

        fs::write(root.join("README.md"), "# Workunit 1660 Fixture\n").unwrap();
        fs::write(root.join("payload.txt"), "ready\n").unwrap();
        fs::write(
            root.join(WORKFLOW),
            "name: Governed\non: [pull_request]\njobs:\n  validate:\n    runs-on: macos-latest\n    steps:\n      - run: decapod validate\n",
        )
        .unwrap();
        let script_body = check_script.unwrap_or(
            "import os\nassert open('payload.txt').read() == 'ready\\n', 'invalid payload'\nassert not os.path.exists(os.environ['WU1660_FAIL_FLAG']), 'observed failure flag'\nprint('check-source-passed')\n",
        );
        fs::write(root.join("scripts/check_source.py"), script_body).unwrap();
        fs::write(
            root.join(".gitignore"),
            ".decapod/data/\n.decapod/managed/governance-document.lock\n.decapod/managed/artifacts/provenance/completion_evidence/\n",
        )
        .unwrap();

        git(&root, &["init", "-b", "main"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "initial source"]);
        git(&upstream, &["init", "--bare"]);
        git(
            &root,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );
        git(&root, &["push", "origin", "main"]);

        let init_out = governed_command(
            &root,
            &storage,
            "wu1660-init",
            &[
                "init",
                "--force",
                "--backend",
                "local",
                "--no-container-workspaces",
            ],
        )
        .bounded_output(Duration::from_secs(60))
        .unwrap();
        assert!(
            init_out.status.success(),
            "decapod init failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&init_out.stdout),
            String::from_utf8_lossy(&init_out.stderr)
        );

        let (program, toolchain_files) = python_runtime();
        let commands = vec![ProofDef {
            name: "inspect-source".into(),
            command: program.clone(),
            args: ["-I", "-S", "-B", "scripts/check_source.py"]
                .map(str::to_owned)
                .into(),
            description: "Inspect repository payload".into(),
            required: true,
        }];
        let execution = RepositoryExecution {
            program,
            toolchain_files,
            platforms: BTreeMap::new(),
            environment: BTreeMap::from([(
                "WU1660_FAIL_FLAG".into(),
                failure_flag.to_string_lossy().into_owned(),
            )]),
            directory: ".".into(),
            timeout_seconds: 15,
        };
        let mut contract = RepositoryContract {
            canonical_local_target: true,
            reviewed_commands_digest: String::new(),
            source_coverage: vec![SourceCoverage {
                source: WORKFLOW.into(),
                commands: vec!["inspect-source".into()],
                ci_only: false,
                reason: None,
            }],
            executions: BTreeMap::from([("inspect-source".into(), execution)]),
            targets: BTreeMap::new(),
        };
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&commands, &contract).unwrap();

        let options = runtime::Options {
            root: root.canonicalize().unwrap(),
            git: Path::new("/usr/bin/git").canonicalize().unwrap(),
            repository: "consumer/project".into(),
            base_branch: "main".into(),
            accepted_base_ref: "refs/remotes/origin/main".into(),
            evidence_parent: evidence.clone(),
            timeout: Duration::from_secs(60),
            max_input_bytes: 64 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
            target: None,
        };
        let sources =
            runtime::discover_source_inventory(&options, &Cancellation::default()).unwrap();
        let proof = ProjectProofConfig {
            observed_path: None,
            selected_work: None,
            validation_buildkite_preflights: false,
            validation_github_actions_preflights: false,
            required_profile: Some(ProfileDeclaration {
                id: repository_profile::REQUIRED_PROFILE.into(),
                reviewed_sources_digest: sources.digest().unwrap(),
            }),
            repository_contract: Some(contract),
            rust_workers: None,
            ci_attested: None,
            commands,
        };

        let mut config: toml::Value =
            toml::from_str(&fs::read_to_string(root.join(CONFIG)).unwrap()).unwrap();
        config["proof"] = toml::Value::try_from(&proof).unwrap();
        config["repo"]["base_branch"] = "main".into();
        fs::write(root.join(CONFIG), toml::to_string_pretty(&config).unwrap()).unwrap();

        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-m", "accept proof config"]);
        git(&root, &["push", "origin", "main"]);

        Self {
            _storage: storage_dir,
            storage,
            root: root.canonicalize().unwrap(),
            _upstream: upstream,
            evidence,
            proof,
            failure_flag,
        }
    }

    fn claim_and_prepare_workspace(
        &mut self,
        agent_id: &str,
        title: &str,
    ) -> (String, String, PathBuf, String) {
        let password = acquire_session(&self.root, &self.storage, agent_id);
        let task = governed_ok(
            &self.root,
            &self.storage,
            agent_id,
            &password,
            &["todo", "add", title, "--format", "json"],
        );
        let task_id = task["id"].as_str().unwrap().to_owned();
        governed_ok(
            &self.root,
            &self.storage,
            agent_id,
            &password,
            &["todo", "claim", "--id", &task_id, "--lease-seconds", "3600"],
        );
        let branch = format!("agent/{agent_id}/{task_id}-proof");
        let ensure = governed_ok(
            &self.root,
            &self.storage,
            agent_id,
            &password,
            &["workspace", "ensure", "--branch", &branch],
        );
        let workspace = PathBuf::from(ensure["worktree_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        governed_ok(
            &workspace,
            &self.storage,
            agent_id,
            &password,
            &["infer", "orientation", "--task-id", &task_id],
        );
        governed_ok(
            &workspace,
            &self.storage,
            agent_id,
            &password,
            &["govern", "plan", "approve"],
        );
        let intent_path = workspace.join(".decapod/managed/specs/INTENT.md");
        let prose = fs::read_to_string(&intent_path)
            .unwrap()
            .replace(
                "Define the user-visible outcome in one paragraph.",
                "Validate committed repository payload through deterministic Python checks before claiming task completion.",
            )
            .replace(
                "against explicit user intent with proof-backed completion.",
                "against explicit repository verification contracts with kernel-observed completion proof.",
            );
        fs::write(
            &intent_path,
            format!("{prose}\n## Workunit proof contract ({task_id})\n\nRequires observed kernel proof before VERIFIED.\n"),
        )
        .unwrap();

        let options = runtime::Options {
            root: workspace.clone(),
            git: Path::new("/usr/bin/git").canonicalize().unwrap(),
            repository: "consumer/project".into(),
            base_branch: "main".into(),
            accepted_base_ref: "refs/remotes/origin/main".into(),
            evidence_parent: self.evidence.clone(),
            timeout: Duration::from_secs(60),
            max_input_bytes: 64 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
            target: None,
        };
        self.proof
            .required_profile
            .as_mut()
            .unwrap()
            .reviewed_sources_digest =
            runtime::discover_source_inventory(&options, &Cancellation::default())
                .unwrap()
                .digest()
                .unwrap();
        let mut config: toml::Value =
            toml::from_str(&fs::read_to_string(workspace.join(CONFIG)).unwrap()).unwrap();
        config["proof"] = toml::Value::try_from(&self.proof).unwrap();
        fs::write(
            workspace.join(CONFIG),
            toml::to_string_pretty(&config).unwrap(),
        )
        .unwrap();
        governed_ok(
            &workspace,
            &self.storage,
            agent_id,
            &password,
            &["rpc", "--op", "specs.refresh"],
        );
        for name in ["ARCHITECTURE", "INTERFACES", "SECURITY"] {
            governed_ok(
                &workspace,
                &self.storage,
                agent_id,
                &password,
                &[
                    "govern",
                    "plan",
                    "review-spec",
                    "--path",
                    &format!(".decapod/managed/specs/{name}.md"),
                    "--disposition",
                    "unchanged-with-reason",
                    "--reason",
                    "Fixture retains baseline architecture, interfaces, and security",
                ],
            );
        }
        git(&workspace, &["add", "-A"]);
        governed_ok(
            &workspace,
            &self.storage,
            agent_id,
            &password,
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                &format!("cp-{task_id}"),
                "--summary",
                "Prepared workunit candidate",
            ],
        );
        git(&workspace, &["add", governance_document::GOVERNANCE_PATH]);
        git(&workspace, &["commit", "-m", "prepare candidate"]);
        governed_ok(
            &workspace,
            &self.storage,
            agent_id,
            &password,
            &["workspace", "ensure", "--branch", &branch],
        );
        assert!(
            git(&workspace, &["status", "--porcelain"]).is_empty(),
            "workspace must be clean"
        );
        (task_id, branch, workspace, password)
    }

    fn init_and_checkpoint_workunit(
        &self,
        workspace: &Path,
        agent_id: &str,
        password: &str,
        task_id: &str,
    ) {
        let gov = governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &["govern", "artifacts", "status"],
        );
        let intent = gov["sections"]["trajectory"]["intent_id"].as_str().unwrap();
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "init",
                "--task-id",
                task_id,
                "--intent-ref",
                intent,
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "attach-spec",
                "--task-id",
                task_id,
                "--ref",
                ".decapod/managed/specs/INTENT.md",
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "capsule",
                "query",
                "--topic",
                "workunit proof",
                "--scope",
                "interfaces",
                "--task-id",
                task_id,
                "--workunit-id",
                task_id,
                "--write",
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "attach-state",
                "--task-id",
                task_id,
                "--ref",
                &format!(".decapod/managed/context/{task_id}.json"),
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "set-proof-plan",
                "--task-id",
                task_id,
                "--gate",
                "validate_passes",
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "transition",
                "--task-id",
                task_id,
                "--to",
                "executing",
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "transition",
                "--task-id",
                task_id,
                "--to",
                "claimed",
            ],
        );
        governed_ok(
            workspace,
            &self.storage,
            agent_id,
            password,
            &[
                "govern",
                "workunit",
                "checkpoint",
                "--task-id",
                task_id,
                "--operation-id",
                "before-proof",
            ],
        );
    }

    fn run_validate(
        &self,
        workspace: &Path,
        agent_id: &str,
        password: &str,
        extra_args: &[&str],
    ) -> (Output, serde_json::Value) {
        let mut args = vec!["validate", "--format", "json"];
        args.extend_from_slice(extra_args);
        let output =
            governed_command_with_pass(workspace, &self.storage, agent_id, password, &args)
                .bounded_output(Duration::from_secs(180))
                .unwrap();
        let value = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
        (output, value)
    }
}

#[test]
fn falsifier_1_self_authored_record_proof_pass_and_sha256_abc_never_verified_or_publishable() {
    let mut fixture = GovernedRepoFixture::new(None);
    let agent = "wu1660-f1";
    let (task_id, branch, workspace, password) =
        fixture.claim_and_prepare_workspace(agent, "Falsifier 1 self-authored pass");
    fixture.init_and_checkpoint_workunit(&workspace, agent, &password, &task_id);

    // Attempt 1a: self-authored pass with synthetic artifact digest sha256:abc (not a regular file).
    let err_synth = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "record-proof",
            "--task-id",
            &task_id,
            "--gate",
            "validate_passes",
            "--status",
            "pass",
            "--artifact",
            "sha256:abc",
        ],
    );
    assert!(
        err_synth.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "expected WORKUNIT_PROOF_UNVERIFIABLE, got:\n{err_synth}"
    );

    // Attempt 1b: even with a real ignored file containing green text, record-proof remains a declaration.
    fs::write(workspace.join(".decapod/data/self_pass.txt"), "PASS\n").unwrap();
    let declared = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "record-proof",
            "--task-id",
            &task_id,
            "--gate",
            "validate_passes",
            "--status",
            "pass",
            "--artifact",
            ".decapod/data/self_pass.txt",
        ],
    );
    assert_eq!(declared["status"], "CLAIMED");
    assert!(declared["validation_record"].is_null());

    // Transitioning to VERIFIED without canonical observed kernel proof must fail closed.
    let err_verified = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_verified.contains("VALIDATION_HISTORY")
            || err_verified.contains("WORKUNIT_PROOF_STALE")
            || err_verified.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "expected proof rejection, got:\n{err_verified}"
    );

    let status = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(status["proof_status"], "not_proven");
    assert_eq!(
        workunit::load_workunit(&workspace, &task_id)
            .unwrap()
            .status,
        workunit::WorkUnitStatus::Claimed
    );

    // Publication gate and workspace publish CLI must both reject.
    let pub_gate_err = ws::verify_workunit_gate_for_publish(&workspace, &branch).unwrap_err();
    assert!(pub_gate_err.to_string().contains("is not VERIFIED"));
    let pub_cli_err = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &["workspace", "publish"],
    );
    assert!(
        !pub_cli_err.is_empty(),
        "workspace publish must fail without verified observed proof"
    );
}

#[test]
fn falsifier_2_unavailable_or_skipped_or_zero_assertion_attempt_stays_not_proven() {
    // 2a & 2b: Partial/quick scope (skips full suite) and failed required execution stay not_proven.
    let mut fixture = GovernedRepoFixture::new(None);
    let agent = "wu1660-f2";
    let (task_id, branch, workspace, password) =
        fixture.claim_and_prepare_workspace(agent, "Falsifier 2 unavailable or skipped attempt");
    fixture.init_and_checkpoint_workunit(&workspace, agent, &password, &task_id);

    // 2a: Partial/quick scope skips the required full suite -> must stay not_proven.
    let (quick_out, quick_json) =
        fixture.run_validate(&workspace, agent, &password, &["--scope", "quick"]);
    assert!(
        !quick_out.status.success() || quick_json["publication_ready"] == false,
        "quick scope cannot certify full proof: {quick_json}"
    );
    let err_after_quick = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_after_quick.contains("VALIDATION_HISTORY")
            || err_after_quick.contains("WORKUNIT_PROOF_STALE")
            || err_after_quick.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "{err_after_quick}"
    );

    // 2b: Full validation with failing required command -> stays not_proven.
    fs::write(&fixture.failure_flag, "unavailable-or-failing-check\n").unwrap();
    let (fail_out, fail_json) = fixture.run_validate(&workspace, agent, &password, &[]);
    assert!(
        !fail_out.status.success(),
        "validate must fail: {fail_json}"
    );
    assert_eq!(fail_json["readiness"]["ready"], false);

    let err_after_fail = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_after_fail.contains("VALIDATION_HISTORY")
            || err_after_fail.contains("WORKUNIT_PROOF_STALE")
            || err_after_fail.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "{err_after_fail}"
    );

    let journal = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(journal["proof_status"], "not_proven");
    assert_eq!(
        workunit::load_workunit(&workspace, &task_id)
            .unwrap()
            .status,
        workunit::WorkUnitStatus::Claimed
    );
    assert!(ws::verify_workunit_gate_for_publish(&workspace, &branch).is_err());

    // 2c: Unavailable required tool/binary (zero executed checks) must stay not_proven.
    let mut unavail_fixture = GovernedRepoFixture::new(Some("print('0 assertions')\n"));
    let unavail_agent = "wu1660-f2c";
    let unavail_exec = unavail_fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .get_mut("inspect-source")
        .unwrap();
    unavail_exec.program = "/nonexistent/decapod-unavailable-tool-1660".into();
    let contract = unavail_fixture.proof.repository_contract.as_mut().unwrap();
    contract.reviewed_commands_digest =
        repository_profile::command_contract_digest(&unavail_fixture.proof.commands, contract)
            .unwrap();
    let mut u_config: toml::Value =
        toml::from_str(&fs::read_to_string(unavail_fixture.root.join(CONFIG)).unwrap()).unwrap();
    u_config["proof"] = toml::Value::try_from(&unavail_fixture.proof).unwrap();
    fs::write(
        unavail_fixture.root.join(CONFIG),
        toml::to_string_pretty(&u_config).unwrap(),
    )
    .unwrap();
    git(&unavail_fixture.root, &["add", CONFIG]);
    git(
        &unavail_fixture.root,
        &["commit", "-m", "accept unavailable tool proof contract"],
    );
    git(&unavail_fixture.root, &["push", "origin", "main"]);
    let (u_task, u_branch, u_ws, u_pass) =
        unavail_fixture.claim_and_prepare_workspace(unavail_agent, "Falsifier 2c unavailable tool");
    unavail_fixture.init_and_checkpoint_workunit(&u_ws, unavail_agent, &u_pass, &u_task);
    let (u_out, u_json) = unavail_fixture.run_validate(&u_ws, unavail_agent, &u_pass, &[]);
    assert!(
        !u_out.status.success(),
        "unavailable tool must fail: {u_json}"
    );
    let u_err = governed_err(
        &u_ws,
        &unavail_fixture.storage,
        unavail_agent,
        &u_pass,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &u_task,
            "--to",
            "verified",
        ],
    );
    assert!(
        u_err.contains("VALIDATION_HISTORY")
            || u_err.contains("WORKUNIT_PROOF_STALE")
            || u_err.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "{u_err}"
    );
    let u_journal = governed_ok(
        &u_ws,
        &unavail_fixture.storage,
        unavail_agent,
        &u_pass,
        &["govern", "workunit", "journal-status", "--task-id", &u_task],
    );
    assert_eq!(u_journal["proof_status"], "not_proven");
    assert!(ws::verify_workunit_gate_for_publish(&u_ws, &u_branch).is_err());
}

#[test]
fn falsifier_3_old_green_receipt_becomes_stale_on_subject_plan_base_or_newer_failure() {
    let mut fixture = GovernedRepoFixture::new(None);
    let agent = "wu1660-f3";
    let (task_id, _branch, workspace, password) =
        fixture.claim_and_prepare_workspace(agent, "Falsifier 3 stale receipt");
    fixture.init_and_checkpoint_workunit(&workspace, agent, &password, &task_id);

    let (ok_out, ok_json) = fixture.run_validate(&workspace, agent, &password, &[]);
    assert!(ok_out.status.success(), "validate failed: {ok_json}");
    assert_eq!(ok_json["readiness"]["ready"], true);

    let verified = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert_eq!(verified["status"], "VERIFIED");
    let status_proven = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(status_proven["proof_status"], "proven");

    // 3a: Subject source mutation invalidates verified manifest -> stale.
    let orig_payload = fs::read_to_string(workspace.join("payload.txt")).unwrap();
    fs::write(workspace.join("payload.txt"), "mutated\n").unwrap();
    let status_mutated = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(status_mutated["proof_status"], "stale", "{status_mutated}");
    let mutated_blocker = status_mutated["resume_blocker"].as_str().unwrap_or("");
    assert!(
        mutated_blocker.contains("EXECUTION_STATE_CHANGED")
            || mutated_blocker.contains("WORKUNIT_PROOF_STALE")
            || mutated_blocker.contains("VALIDATION_HISTORY"),
        "{status_mutated}"
    );
    fs::write(workspace.join("payload.txt"), orig_payload).unwrap();

    // 3b: Accepted base advancement invalidates receipt -> stale.
    let old_base = git(&workspace, &["rev-parse", "refs/remotes/origin/main"]);
    let head_sha = git(&workspace, &["rev-parse", "HEAD"]);
    git(
        &workspace,
        &["update-ref", "refs/remotes/origin/main", &head_sha],
    );
    let status_base = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(status_base["proof_status"], "stale", "{status_base}");
    git(
        &workspace,
        &["update-ref", "refs/remotes/origin/main", &old_base],
    );

    // 3c: Newer failed validation withdraws VERIFIED -> CLAIMED and marks stale.
    fs::write(&fixture.failure_flag, "newer-failure\n").unwrap();
    let (fail_out, _) = fixture.run_validate(&workspace, agent, &password, &[]);
    assert!(!fail_out.status.success());

    let after_fail = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &["govern", "workunit", "get", "--task-id", &task_id],
    );
    assert_eq!(after_fail["status"], "CLAIMED");
    assert!(after_fail["validation_record"].is_null());

    let status_withdrawn = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(status_withdrawn["proof_status"], "stale");

    let err_repromote = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_repromote.contains("VALIDATION_HISTORY")
            || err_repromote.contains("WORKUNIT_PROOF_STALE"),
        "{err_repromote}"
    );

    // 3d: Changing proof_plan clears old verification and rejects transition.
    fs::remove_file(&fixture.failure_flag).unwrap();
    governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "set-proof-plan",
            "--task-id",
            &task_id,
            "--gate",
            "validated_commit",
        ],
    );
    let err_plan = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_plan.contains("VALIDATION_HISTORY") || err_plan.contains("WORKUNIT_PROOF_STALE"),
        "changed plan must fail validation proof check, got:\n{err_plan}"
    );
}

#[test]
fn falsifier_4_authentic_proof_copied_from_another_task_repo_or_custody_generation_rejected() {
    let mut fixture = GovernedRepoFixture::new(None);
    let agent_a = "wu1660-f4a";
    let (task_a, _branch_a, workspace_a, pass_a) =
        fixture.claim_and_prepare_workspace(agent_a, "Falsifier 4 task A");
    fixture.init_and_checkpoint_workunit(&workspace_a, agent_a, &pass_a, &task_a);

    let (ok_out, _) = fixture.run_validate(&workspace_a, agent_a, &pass_a, &[]);
    assert!(ok_out.status.success());
    let verified_a = governed_ok(
        &workspace_a,
        &fixture.storage,
        agent_a,
        &pass_a,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_a,
            "--to",
            "verified",
        ],
    );
    assert_eq!(verified_a["status"], "VERIFIED");
    let manifest_a: workunit::WorkUnitManifest =
        serde_json::from_value(verified_a.clone()).unwrap();

    // 4a: Copy authentic VERIFIED proof from task_a onto task_b in another workspace.
    let agent_b = "wu1660-f4b";
    let (task_b, branch_b, workspace_b, pass_b) =
        fixture.claim_and_prepare_workspace(agent_b, "Falsifier 4 task B");
    fixture.init_and_checkpoint_workunit(&workspace_b, agent_b, &pass_b, &task_b);

    let mut forged_for_b = manifest_a.clone();
    forged_for_b.task_id = task_b.clone();
    forged_for_b.state_refs = vec![format!(".decapod/managed/context/{task_b}.json")];
    let path_b = workunit::workunit_path(&workspace_b, &task_b).unwrap();
    fs::write(&path_b, serde_json::to_vec_pretty(&forged_for_b).unwrap()).unwrap();

    let err_get_b = governed_err(
        &workspace_b,
        &fixture.storage,
        agent_b,
        &pass_b,
        &["govern", "workunit", "get", "--task-id", &task_b],
    );
    assert!(
        err_get_b.contains("WORKUNIT_RECOVERY_REQUIRED"),
        "{err_get_b}"
    );

    let err_checked = workunit::write_workunit_checked(
        &workspace_b,
        &forged_for_b,
        "copy-task-a-proof",
        workunit::workunit_journal_status(&workspace_b, &task_b)
            .unwrap()
            .generation
            .unwrap(),
    )
    .unwrap_err();
    assert!(
        err_checked
            .to_string()
            .contains("WORKUNIT_DOMAIN_TRANSITION_REQUIRED"),
        "{err_checked}"
    );
    assert!(ws::verify_workunit_gate_for_publish(&workspace_b, &branch_b).is_err());

    // Recover must preserve unrecognized forged JSON (`WORKUNIT_PROJECTION_CONFLICT`) until explicitly removed.
    let err_recover_conflict = governed_err(
        &workspace_b,
        &fixture.storage,
        agent_b,
        &pass_b,
        &["govern", "workunit", "recover", "--task-id", &task_b],
    );
    assert!(
        err_recover_conflict.contains("WORKUNIT_PROJECTION_CONFLICT"),
        "{err_recover_conflict}"
    );
    fs::remove_file(&path_b).unwrap();
    governed_ok(
        &workspace_b,
        &fixture.storage,
        agent_b,
        &pass_b,
        &["govern", "workunit", "recover", "--task-id", &task_b],
    );
    let err_trans_b = governed_err(
        &workspace_b,
        &fixture.storage,
        agent_b,
        &pass_b,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_b,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_trans_b.contains("VALIDATION_HISTORY")
            || err_trans_b.contains("WORKUNIT_PROOF_STALE")
            || err_trans_b.contains("WORKUNIT_PROOF_UNVERIFIABLE"),
        "{err_trans_b}"
    );
}

#[test]
fn falsifier_4b_cross_custody_and_cross_repo_proof_are_rejected() {
    let mut fixture_a = GovernedRepoFixture::new(None);
    let agent_a = "wu1660-f4x";
    let (task_a, branch_a, workspace_a, pass_a) =
        fixture_a.claim_and_prepare_workspace(agent_a, "Falsifier 4 custody & cross-repo");
    fixture_a.init_and_checkpoint_workunit(&workspace_a, agent_a, &pass_a, &task_a);

    let (ok_out, _) = fixture_a.run_validate(&workspace_a, agent_a, &pass_a, &[]);
    assert!(ok_out.status.success());
    let verified_a = governed_ok(
        &workspace_a,
        &fixture_a.storage,
        agent_a,
        &pass_a,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_a,
            "--to",
            "verified",
        ],
    );
    assert_eq!(verified_a["status"], "VERIFIED");

    // Yield and reclaim task_a under new custody generation -> old VERIFIED proof is no longer valid.
    governed_ok(
        &workspace_a,
        &fixture_a.storage,
        agent_a,
        &pass_a,
        &["todo", "yield", "--id", &task_a, "--reason", "custody-bump"],
    );
    let agent_reclaim = "wu1660-f4y";
    let pass_reclaim = acquire_session(&fixture_a.root, &fixture_a.storage, agent_reclaim);
    governed_ok(
        &fixture_a.root,
        &fixture_a.storage,
        agent_reclaim,
        &pass_reclaim,
        &["todo", "claim", "--id", &task_a, "--lease-seconds", "3600"],
    );
    let err_other_custodian = governed_err(
        &workspace_a,
        &fixture_a.storage,
        agent_reclaim,
        &pass_reclaim,
        &["govern", "workunit", "journal-status", "--task-id", &task_a],
    );
    assert!(
        err_other_custodian.contains("AUTHORING_BINDING_REQUIRED"),
        "{err_other_custodian}"
    );
    assert!(ws::verify_workunit_gate_for_publish(&workspace_a, &branch_a).is_err());

    // Even if the original agent reclaims task_a (ABA new lease_generation), prior VERIFIED proof is stale.
    governed_ok(
        &fixture_a.root,
        &fixture_a.storage,
        agent_reclaim,
        &pass_reclaim,
        &["todo", "yield", "--id", &task_a, "--reason", "aba-reclaim"],
    );
    governed_ok(
        &fixture_a.root,
        &fixture_a.storage,
        agent_a,
        &pass_a,
        &["todo", "claim", "--id", &task_a, "--lease-seconds", "3600"],
    );
    let err_after_aba = governed_err(
        &workspace_a,
        &fixture_a.storage,
        agent_a,
        &pass_a,
        &["govern", "workunit", "journal-status", "--task-id", &task_a],
    );
    assert!(
        err_after_aba.contains("AUTHORING_BINDING_REQUIRED"),
        "{err_after_aba}"
    );
    let status_after_reclaim = workunit::workunit_journal_status(&workspace_a, &task_a).unwrap();
    assert!(!status_after_reclaim.resumable);
    assert_eq!(
        status_after_reclaim.proof_status, "stale",
        "{status_after_reclaim:?}"
    );
    assert!(ws::verify_workunit_gate_for_publish(&workspace_a, &branch_a).is_err());

    // Cross-repo copy: write Repo A's manifest into an independent Repo B and check validation fails.
    let mut fixture_b = GovernedRepoFixture::new(None);
    let agent_b = "wu1660-f4z";
    let (task_b, branch_b, workspace_b, pass_b) =
        fixture_b.claim_and_prepare_workspace(agent_b, "Falsifier 4 repo B");
    fixture_b.init_and_checkpoint_workunit(&workspace_b, agent_b, &pass_b, &task_b);
    let manifest_a: workunit::WorkUnitManifest = serde_json::from_value(verified_a).unwrap();
    let mut cross_repo = manifest_a;
    cross_repo.task_id = task_b.clone();
    cross_repo.state_refs = vec![format!(".decapod/managed/context/{task_b}.json")];
    let path_b = workunit::workunit_path(&workspace_b, &task_b).unwrap();
    fs::write(&path_b, serde_json::to_vec_pretty(&cross_repo).unwrap()).unwrap();
    assert!(ws::verify_workunit_gate_for_publish(&workspace_b, &branch_b).is_err());
    let err_b = governed_err(
        &workspace_b,
        &fixture_b.storage,
        agent_b,
        &pass_b,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_b,
            "--to",
            "verified",
        ],
    );
    assert!(!err_b.is_empty());
}

#[test]
fn falsifier_5_and_6_split_store_retained_as_not_proven_audit_and_converged_task_completes_verified()
 {
    let mut fixture = GovernedRepoFixture::new(None);
    let agent = "wu1660-f56";
    let (task_id, branch, workspace, password) =
        fixture.claim_and_prepare_workspace(agent, "Falsifier 5 and 6 split store convergence");

    // Falsifier (5): populate a separately initialized workspace-local datastore inside the linked worktree
    // using an independent standalone repository initialized via the real decapod CLI.
    let split_origin = fixture.storage.join("split-origin");
    fs::create_dir_all(&split_origin).unwrap();
    git(&split_origin, &["init", "-b", "main"]);
    fs::write(split_origin.join("README.md"), "# Split Origin\n").unwrap();
    git(&split_origin, &["add", "."]);
    git(&split_origin, &["commit", "-m", "init split origin"]);
    let split_init = governed_command(
        &split_origin,
        &fixture.storage,
        agent,
        &[
            "init",
            "--force",
            "--backend",
            "local",
            "--no-container-workspaces",
        ],
    )
    .bounded_output(Duration::from_secs(60))
    .unwrap();
    assert!(
        split_init.status.success(),
        "split origin init failed: {}",
        String::from_utf8_lossy(&split_init.stderr)
    );
    decapod::plugins::policy::initialize_policy_db(&split_origin.join(".decapod/data")).unwrap();
    decapod::core::todo::initialize_todo_db(&split_origin.join(".decapod/data")).unwrap();
    // Record local split-store workunit history (including self-declared sha256:abc and local VERIFIED projection).
    let split_task = "bugs_local_split_01";
    let split_verified_task = "bugs_local_split_02";
    workunit::init_workunit(&split_origin, split_task, "intent:split-local").unwrap();
    workunit::add_spec_ref(
        &split_origin,
        split_task,
        ".decapod/managed/specs/INTENT.md",
    )
    .unwrap();
    workunit::set_proof_plan(&split_origin, split_task, &["validate_passes".to_string()]).unwrap();
    workunit::transition_status(
        &split_origin,
        split_task,
        workunit::WorkUnitStatus::Executing,
    )
    .unwrap();
    workunit::transition_status(&split_origin, split_task, workunit::WorkUnitStatus::Claimed)
        .unwrap();
    workunit::write_workunit(
        &split_origin,
        &workunit::WorkUnitManifest {
            task_id: split_verified_task.into(),
            intent_ref: "intent:split-verified".into(),
            spec_refs: vec![".decapod/managed/specs/INTENT.md".into()],
            state_refs: vec![],
            proof_plan: vec!["validate_passes".into()],
            proof_results: vec![workunit::WorkUnitProofResult {
                gate: "validate_passes".into(),
                status: "pass".into(),
                artifact_ref: Some("sha256:abc".into()),
                evaluator_epoch: None,
                validation_epoch: None,
            }],
            validation_epoch: None,
            validation_record: None,
            status: workunit::WorkUnitStatus::Verified,
        },
    )
    .unwrap();

    // Copy the independently initialized .decapod/data store into the linked worktree.
    let ws_data = workspace.join(".decapod/data");
    fs::create_dir_all(ws_data.join("workunits")).unwrap();
    for file_name in [
        "decapod.db",
        "decapod.db-wal",
        "decapod.db-shm",
        "todo.db",
        "todo.db-wal",
        "todo.db-shm",
    ] {
        let src = split_origin.join(".decapod/data").join(file_name);
        if src.is_file() {
            fs::copy(&src, ws_data.join(file_name)).unwrap();
        }
    }
    for entry in fs::read_dir(split_origin.join(".decapod/data/workunits")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            fs::copy(
                &path,
                ws_data.join("workunits").join(path.file_name().unwrap()),
            )
            .unwrap();
        }
    }
    assert!(workspace.join(".decapod/data/decapod.db").is_file());

    let split_verified_status = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            split_verified_task,
        ],
    );
    assert_eq!(split_verified_status["split_store"], true);
    assert_eq!(split_verified_status["proof_status"], "not_proven");
    assert_eq!(split_verified_status["resumable"], false);
    assert!(
        workunit::validate_verified_manifest(
            &workspace,
            &workunit::load_workunit(&workspace, split_verified_task).unwrap()
        )
        .unwrap_err()
        .to_string()
        .contains("WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED")
    );
    // Resume on a split store must fail closed with WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED.
    let err_split_resume = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "resume",
            "--task-id",
            split_verified_task,
            "--operation-id",
            "split-resume-blocked",
        ],
    );
    assert!(
        err_split_resume.contains("WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED"),
        "{err_split_resume}"
    );
    // Non-VERIFIED declarations on an explicitly initialized workspace-local
    // datastore stay isolated in `<workspace>/.decapod/data` (never leaking into
    // the canonical main store) and still cannot transition to VERIFIED.
    governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "bugs_split_local_only",
            "--intent-ref",
            "intent:split-local-only",
        ],
    );
    assert!(
        workspace
            .join(".decapod/data/workunits/bugs_split_local_only.json")
            .is_file()
    );
    assert!(
        !fixture
            .root
            .join(".decapod/data/workunits/bugs_split_local_only.json")
            .exists(),
        "split-store local workunit must never leak into the canonical main store"
    );

    // Inspection must flag split store as not_proven with WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED.
    let split_status = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            split_task,
        ],
    );
    assert_eq!(split_status["split_store"], true);
    assert_eq!(split_status["atomic_domain_supported"], false);
    assert_eq!(split_status["resumable"], false);
    assert_eq!(split_status["proof_status"], "not_proven");
    assert!(
        split_status["resume_blocker"]
            .as_str()
            .unwrap_or("")
            .contains("WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED"),
        "{split_status}"
    );
    assert!(!split_status["audit_records"].as_array().unwrap().is_empty());

    // Transitioning to VERIFIED in the split store must fail with WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED.
    let err_split_verify = governed_err(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            split_task,
            "--to",
            "verified",
        ],
    );
    assert!(
        err_split_verify.contains("WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED"),
        "expected WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED, got:\n{err_split_verify}"
    );

    // Dry-run convergence inspects without mutating.
    let dry_converge = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &["govern", "workunit", "converge", "--dry-run"],
    );
    assert_eq!(dry_converge["dry_run"], true);
    assert_eq!(dry_converge["split_store"], true);
    assert_eq!(dry_converge["proof_status"], "not_proven");
    assert!(workspace.join(".decapod/data/decapod.db").is_file());

    // Explicit convergence archives workspace-local store as not_proven audit history.
    let converged = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &["govern", "workunit", "converge"],
    );
    assert_eq!(converged["dry_run"], false);
    assert_eq!(converged["split_store"], false);
    assert_eq!(converged["atomic_domain_supported"], true);
    assert_eq!(converged["proof_status"], "not_proven");
    let archive_dir = PathBuf::from(converged["archive_dir"].as_str().unwrap());
    assert!(archive_dir.join("audit.json").is_file());
    assert!(!workspace.join(".decapod/data/decapod.db").exists());

    // Split-store historical tasks must NOT be promoted into canonical active workunits,
    // and remain inspectable as not_proven audit history.
    assert!(workunit::load_workunit(&workspace, split_task).is_err());
    assert!(workunit::load_workunit(&workspace, split_verified_task).is_err());
    let post_converge_audit = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            split_task,
        ],
    );
    assert_eq!(post_converge_audit["split_store"], false);
    assert_eq!(post_converge_audit["atomic_domain_supported"], true);
    assert_eq!(post_converge_audit["resumable"], false);
    assert_eq!(post_converge_audit["proof_status"], "not_proven");
    assert_eq!(
        post_converge_audit["projection_state"],
        "split_audit_archived"
    );
    let audit_rows = post_converge_audit["audit_records"].as_array().unwrap();
    assert!(!audit_rows.is_empty());
    assert!(
        audit_rows
            .iter()
            .all(|row| row["proof_status"] == "not_proven"
                && row["classification"] == "WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED"),
        "{post_converge_audit}"
    );

    // Falsifier (6): repaired current task on the converged workspace completes VERIFIED atomically.
    // Re-sync workspace config/specs cleanly (split init touched .decapod/config.toml/specs if modified).
    git(
        &workspace,
        &[
            "checkout",
            "--",
            ".decapod/config.toml",
            ".decapod/managed/",
        ],
    );
    assert!(git(&workspace, &["status", "--porcelain"]).is_empty());

    fixture.init_and_checkpoint_workunit(&workspace, agent, &password, &task_id);
    let (ok_out, ok_json) = fixture.run_validate(&workspace, agent, &password, &[]);
    assert!(
        ok_out.status.success(),
        "repaired validate failed: {ok_json}"
    );
    assert_eq!(ok_json["readiness"]["ready"], true);
    assert_eq!(ok_json["readiness"]["proof_ready"], true);

    let verified = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert_eq!(verified["status"], "VERIFIED");
    assert!(verified["validation_record"].is_object());

    let final_journal = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &[
            "govern",
            "workunit",
            "journal-status",
            "--task-id",
            &task_id,
        ],
    );
    assert_eq!(final_journal["proof_status"], "proven");
    assert_eq!(final_journal["split_store"], false);
    assert_eq!(final_journal["atomic_domain_supported"], true);
    assert_eq!(final_journal["resumable"], true);

    let final_status = governed_ok(
        &workspace,
        &fixture.storage,
        agent,
        &password,
        &["govern", "workunit", "status", "--task-id", &task_id],
    );
    assert_eq!(final_status["workunit_status"], "VERIFIED");
    assert_eq!(final_status["proof_status"], "proven");

    // Execute the in-process publish gate under the owning custodian agent ID.
    let prior_agent = std::env::var_os("DECAPOD_AGENT_ID");
    // SAFETY: test suite runs single-threaded for env-scoped agent identity check and restores prior value immediately.
    unsafe {
        std::env::set_var("DECAPOD_AGENT_ID", agent);
    }
    let publish_gate_result = ws::verify_workunit_gate_for_publish(&workspace, &branch);
    // SAFETY: restoring previous DECAPOD_AGENT_ID value after publish gate verification.
    unsafe {
        match prior_agent {
            Some(prev) => std::env::set_var("DECAPOD_AGENT_ID", prev),
            None => std::env::remove_var("DECAPOD_AGENT_ID"),
        }
    }
    publish_gate_result.expect("repaired canonical VERIFIED workunit must pass publish gate");
}
