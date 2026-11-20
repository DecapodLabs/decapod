//! Actual-binary continuous-governance and exact-candidate regression suite (#1052).
//!
//! Exercises the compiled `decapod` binary (`CARGO_BIN_EXE_decapod`) end-to-end
//! in isolated disposable Git repositories to verify:
//! 1. Exact candidate binding across SHA, tree, accepted base, task, and lease generation.
//! 2. Immediate proof invalidation after uncommitted edits, new commits, tree-preserving
//!    or tree-modifying amend/rebase, accepted-base advancement, task lease renewal
//!    (`lease_generation`), and partial/`ci_local` scopes (`GITHUB_ACTIONS`, `BUILDKITE`,
//!    `govern project-check`).
//! 3. Truthful immutable `decapod qa verify commit <sha>` lookup that retains historical
//!    observations without granting readiness to a newer, amended, rebased, or
//!    custody-changed candidate, plus `workspace publish` freshness refusals.

#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::proof::ProofDef;
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime::{self as runtime};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const AGENT_ID: &str = "agent-exact-1052";
const CHECK_WORKFLOW: &str = ".github/workflows/ci.yml";

struct EnvOutput {
    output: Output,
    combined: String,
}

impl EnvOutput {
    fn success(&self) -> bool {
        self.output.status.success()
    }

    fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout)
            .trim()
            .to_string()
    }

    fn json(&self) -> Value {
        let raw = self.stdout_str();
        serde_json::from_str(&raw).unwrap_or_else(|err| {
            panic!(
                "expected valid JSON on stdout ({err}); combined output:\n{}",
                self.combined
            )
        })
    }
}

struct ExactCandidateHarness {
    _temp: TempDir,
    home: PathBuf,
    xdg_config: PathBuf,
    xdg_data: PathBuf,
    xdg_state: PathBuf,
    xdg_cache: PathBuf,
    repo_root: PathBuf,
    origin_bare: Option<PathBuf>,
    worktree: PathBuf,
    branch: String,
    task_id: String,
    session_password: String,
    base_sha: String,
}

fn scrub_command_env(cmd: &mut Command, harness_home: Option<&Path>, xdg: Option<[&Path; 4]>) {
    let remove_keys: Vec<String> = std::env::vars()
        .map(|(k, _)| k)
        .filter(|k| {
            k.starts_with("DECAPOD_")
                || k.starts_with("GIT_")
                || k.starts_with("GITHUB_")
                || k.starts_with("BUILDKITE")
                || k.starts_with("CHANGE_")
                || k.starts_with("SOURCE_CI_")
                || k.starts_with("XDG_")
        })
        .collect();
    for key in remove_keys {
        cmd.env_remove(key);
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(home) = harness_home {
        cmd.env("HOME", home);
    }
    if let Some([cfg, data, state, cache]) = xdg {
        cmd.env("XDG_CONFIG_HOME", cfg)
            .env("XDG_DATA_HOME", data)
            .env("XDG_STATE_HOME", state)
            .env("XDG_CACHE_HOME", cache);
    }
}

fn run_git_raw(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new("git");
    scrub_command_env(&mut cmd, None, None);
    cmd.current_dir(dir).args(args).output().expect("run git")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = run_git_raw(dir, args);
    assert!(
        out.status.success(),
        "git {:?} in {} failed:\nstdout:\n{}\nstderr:\n{}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_decapod_raw(
    dir: &Path,
    home: &Path,
    xdg: [&Path; 4],
    session_password: Option<&str>,
    args: &[&str],
    extra_envs: &[(&str, &str)],
) -> EnvOutput {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    scrub_command_env(&mut cmd, Some(home), Some(xdg));
    cmd.current_dir(dir)
        .args(args)
        .env("DECAPOD_AGENT_ID", AGENT_ID)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS", "180000");
    if let Some(pw) = session_password {
        cmd.env("DECAPOD_SESSION_PASSWORD", pw);
    }
    for (k, v) in extra_envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("execute CARGO_BIN_EXE_decapod");
    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    EnvOutput { output, combined }
}

fn find_in_path(binary: &str) -> PathBuf {
    let path_var = std::env::var_os("PATH").expect("PATH must be set");
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return candidate.canonicalize().unwrap_or(candidate);
        }
    }
    panic!("could not find {binary} in PATH");
}

fn write_consumer_proof_config(repo_root: &Path, canonical_local_target: bool) {
    let config_path = repo_root.join(".decapod/config.toml");
    let existing = fs::read_to_string(&config_path).expect("read initialized config.toml");
    let mut config: toml::Value = toml::from_str(&existing).expect("parse config.toml");

    let root_tbl = config.as_table_mut().expect("config root table");
    let repo_tbl = root_tbl
        .entry("repo")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .expect("repo table");
    repo_tbl.insert("backend".into(), toml::Value::String("local".into()));
    repo_tbl.insert("base_branch".into(), toml::Value::String("main".into()));
    repo_tbl.insert(
        "canonical_local_target".into(),
        toml::Value::Boolean(canonical_local_target),
    );

    let ws_tbl = root_tbl
        .entry("workspace")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .expect("workspace table");
    ws_tbl.insert("container_workspaces".into(), toml::Value::Boolean(false));

    let test_bin = find_in_path("test");
    let test_str = test_bin.to_string_lossy().to_string();
    let mut toolchain_files = BTreeMap::new();
    toolchain_files.insert("test".to_string(), test_str.clone());

    let commands = vec![ProofDef {
        name: "smoke".to_string(),
        command: test_str.clone(),
        args: vec!["-f".to_string(), "app.txt".to_string()],
        description: "Verify consumer app marker exists".to_string(),
        required: true,
    }];

    let mut executions = BTreeMap::new();
    executions.insert(
        "smoke".to_string(),
        RepositoryExecution {
            program: test_str,
            toolchain_files,
            platforms: BTreeMap::new(),
            environment: BTreeMap::new(),
            directory: ".".to_string(),
            timeout_seconds: 30,
        },
    );

    let coverage = vec![SourceCoverage {
        source: CHECK_WORKFLOW.to_string(),
        commands: vec!["smoke".to_string()],
        ci_only: false,
        reason: None,
    }];

    let mut contract = RepositoryContract {
        canonical_local_target,
        reviewed_commands_digest: String::new(),
        source_coverage: coverage,
        executions,
        targets: Default::default(),
    };
    contract.reviewed_commands_digest =
        repository_profile::command_contract_digest(&commands, &contract)
            .expect("compute reviewed_commands_digest");

    let sources = runtime::discover_source_inventory(
        &runtime::Options {
            root: repo_root.canonicalize().expect("canonicalize repo root"),
            git: find_in_path("git"),
            repository: "local/exact-candidate".to_string(),
            base_branch: "main".to_string(),
            accepted_base_ref: if canonical_local_target {
                "refs/heads/main".to_string()
            } else {
                "refs/remotes/origin/main".to_string()
            },
            evidence_parent: std::env::temp_dir().canonicalize().expect("temp_dir"),
            timeout: std::time::Duration::from_secs(60),
            max_input_bytes: 8 * 1024 * 1024 * 1024,
            max_files: 200_000,
            max_output_bytes: 16 * 1024 * 1024,
            target: None,
        },
        &decapod::core::bounded_process::Cancellation::default(),
    )
    .expect("discover source inventory");

    let proof_tbl = root_tbl
        .entry("proof")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .expect("proof table");
    proof_tbl.insert(
        "validation_buildkite_preflights".into(),
        toml::Value::Boolean(false),
    );
    proof_tbl.insert(
        "commands".into(),
        toml::Value::try_from(&commands).expect("serialize commands"),
    );
    proof_tbl.insert(
        "repository_contract".into(),
        toml::Value::try_from(&contract).expect("serialize contract"),
    );
    proof_tbl.insert(
        "required_profile".into(),
        toml::Value::try_from(&ProfileDeclaration {
            id: repository_profile::REQUIRED_PROFILE.to_string(),
            reviewed_sources_digest: sources.digest().expect("sources digest"),
        })
        .expect("serialize required_profile"),
    );

    let serialized = toml::to_string_pretty(&config).expect("serialize config.toml");
    fs::write(&config_path, serialized).expect("write config.toml");
}

fn update_operations_spec(root: &Path, marker: &str) {
    let path = root.join(".decapod/managed/specs/OPERATIONS.md");
    let existing = fs::read_to_string(&path).expect("read OPERATIONS.md");
    let updated = format!(
        "{existing}\n\n## Exact Candidate Note ({marker})\n\nDocumented exact candidate validation contract for {marker}.\n"
    );
    fs::write(&path, updated).expect("write OPERATIONS.md");
}

impl ExactCandidateHarness {
    fn new(canonical_local_target: bool) -> Self {
        let temp = TempDir::new().expect("tmpdir");
        let root_tmp = temp.path();
        let home = root_tmp.join("home");
        let xdg_config = root_tmp.join("xdg_config");
        let xdg_data = root_tmp.join("xdg_data");
        let xdg_state = root_tmp.join("xdg_state");
        let xdg_cache = root_tmp.join("xdg_cache");
        for dir in [&home, &xdg_config, &xdg_data, &xdg_state, &xdg_cache] {
            fs::create_dir_all(dir).expect("create isolated dir");
        }
        let xdg = [
            xdg_config.as_path(),
            xdg_data.as_path(),
            xdg_state.as_path(),
            xdg_cache.as_path(),
        ];

        let repo_root = root_tmp.join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        git(&repo_root, &["init", "-b", "main"]);
        git(&repo_root, &["config", "user.name", "Exact Candidate Test"]);
        git(
            &repo_root,
            &["config", "user.email", "exact@example.invalid"],
        );

        let origin_bare = if canonical_local_target {
            None
        } else {
            let bare = root_tmp.join("origin.git");
            fs::create_dir_all(&bare).expect("create bare origin");
            git(&bare, &["init", "--bare", "-b", "main"]);
            // Preserve local bare transport while giving the repository resolver a canonical identity.
            git(
                &bare,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/consumer/exact-candidate.git",
                ],
            );
            git(
                &repo_root,
                &["remote", "add", "origin", bare.to_str().unwrap()],
            );
            Some(bare)
        };

        let init = run_decapod_raw(
            &repo_root,
            &home,
            xdg,
            None,
            &["init", "--force", "--no-container-workspaces", "--no-ci"],
            &[],
        );
        assert!(init.success(), "decapod init failed:\n{}", init.combined);

        let intent_path = repo_root.join(".decapod/managed/specs/INTENT.md");
        let intent_raw = fs::read_to_string(&intent_path).expect("read INTENT.md");
        fs::write(
            &intent_path,
            intent_raw.replace(
                "Define the user-visible outcome in one paragraph.",
                "Verify exact-candidate governance and continuous proof validity across disposable consumer commits.",
            ),
        )
        .expect("write INTENT.md");

        fs::create_dir_all(repo_root.join(".github/workflows")).expect("create workflows");
        fs::write(
            repo_root.join(CHECK_WORKFLOW),
            "name: Consumer CI\non: [push, pull_request]\njobs:\n  smoke:\n    runs-on: ubuntu-latest\n    steps:\n      - run: test -f app.txt\n",
        )
        .expect("write workflow");
        fs::create_dir_all(repo_root.join("src")).expect("create src");
        fs::write(repo_root.join("app.txt"), "base-v1\n").expect("write app.txt");
        fs::write(repo_root.join("src/app.txt"), "module-v1\n").expect("write src/app.txt");

        git(&repo_root, &["add", "."]);
        git(&repo_root, &["commit", "-m", "chore: seed initial base"]);
        if origin_bare.is_some() {
            git(&repo_root, &["push", "-u", "origin", "main"]);
        }

        write_consumer_proof_config(&repo_root, canonical_local_target);

        let acquire = run_decapod_raw(&repo_root, &home, xdg, None, &["session", "acquire"], &[]);
        assert!(
            acquire.success(),
            "session acquire failed:\n{}",
            acquire.combined
        );
        let session_password = acquire
            .combined
            .lines()
            .find_map(|line| {
                line.strip_prefix("Password: ")
                    .map(|s| s.trim().to_string())
            })
            .expect("session password from session acquire");

        git(&repo_root, &["add", "."]);
        git(
            &repo_root,
            &["commit", "-m", "chore: configure proof target"],
        );
        if origin_bare.is_some() {
            git(&repo_root, &["push", "origin", "main"]);
        }
        let base_sha = git(&repo_root, &["rev-parse", "HEAD"]);

        let todo_add = run_decapod_raw(
            &repo_root,
            &home,
            xdg,
            Some(&session_password),
            &[
                "todo",
                "add",
                "Exact candidate verification task",
                "--format",
                "json",
            ],
            &[],
        );
        assert!(
            todo_add.success(),
            "todo add failed:\n{}",
            todo_add.combined
        );
        let todo_json = todo_add.json();
        let task_id = todo_json["id"]
            .as_str()
            .or_else(|| todo_json["result"]["id"].as_str())
            .expect("todo id")
            .to_string();

        let mut todo_claim = run_decapod_raw(
            &repo_root,
            &home,
            xdg,
            Some(&session_password),
            &["todo", "claim", "--id", &task_id, "--format", "json"],
            &[],
        );
        if !todo_claim.success() && todo_claim.combined.contains("BROKER_UNKNOWN") {
            let check = run_decapod_raw(
                &repo_root,
                &home,
                xdg,
                Some(&session_password),
                &["todo", "get", "--id", &task_id, "--format", "json"],
                &[],
            );
            let owner_ok =
                check.success() && check.json()["todo"]["owner"].as_str() == Some(AGENT_ID);
            if !owner_ok {
                todo_claim = run_decapod_raw(
                    &repo_root,
                    &home,
                    xdg,
                    Some(&session_password),
                    &["todo", "claim", "--id", &task_id, "--format", "json"],
                    &[],
                );
                assert!(
                    todo_claim.success(),
                    "todo claim retry failed:\n{}",
                    todo_claim.combined
                );
            }
        } else {
            assert!(
                todo_claim.success(),
                "todo claim failed:\n{}",
                todo_claim.combined
            );
        }

        let branch = format!("agent/{AGENT_ID}/{task_id}-exact-candidate");
        let ensure = run_decapod_raw(
            &repo_root,
            &home,
            xdg,
            Some(&session_password),
            &["workspace", "ensure", "--branch", &branch],
            &[],
        );
        assert!(
            ensure.success(),
            "workspace ensure failed:\n{}",
            ensure.combined
        );
        let ensure_json = ensure.json();
        let worktree_str = ensure_json["worktree_path"]
            .as_str()
            .unwrap_or_else(|| panic!("missing worktree_path in ensure: {ensure_json}"));
        let worktree = PathBuf::from(worktree_str);

        Self {
            _temp: temp,
            home,
            xdg_config,
            xdg_data,
            xdg_state,
            xdg_cache,
            repo_root,
            origin_bare,
            worktree,
            branch,
            task_id,
            session_password,
            base_sha,
        }
    }

    fn xdg(&self) -> [&Path; 4] {
        [
            self.xdg_config.as_path(),
            self.xdg_data.as_path(),
            self.xdg_state.as_path(),
            self.xdg_cache.as_path(),
        ]
    }

    fn decapod(&self, args: &[&str]) -> EnvOutput {
        run_decapod_raw(
            &self.worktree,
            &self.home,
            self.xdg(),
            Some(&self.session_password),
            args,
            &[],
        )
    }

    fn decapod_with_env(&self, args: &[&str], extra_envs: &[(&str, &str)]) -> EnvOutput {
        run_decapod_raw(
            &self.worktree,
            &self.home,
            self.xdg(),
            Some(&self.session_password),
            args,
            extra_envs,
        )
    }

    fn decapod_in(&self, dir: &Path, args: &[&str]) -> EnvOutput {
        run_decapod_raw(
            dir,
            &self.home,
            self.xdg(),
            Some(&self.session_password),
            args,
            &[],
        )
    }

    fn prepare_governed_worktree_commit(
        &self,
        src_content: &str,
        spec_marker: &str,
        checkpoint_id: &str,
        commit_msg: &str,
        plan_configured: bool,
    ) -> String {
        fs::write(self.worktree.join("src/app.txt"), src_content).expect("write src/app.txt");
        update_operations_spec(&self.worktree, spec_marker);

        let refresh = self.decapod(&["rpc", "--op", "specs.refresh"]);
        assert!(
            refresh.success(),
            "specs.refresh in worktree failed:\n{}",
            refresh.combined
        );

        let gov_raw = fs::read_to_string(self.worktree.join(".decapod/governance.json"))
            .expect("read governance.json");
        let gov_json: Value = serde_json::from_str(&gov_raw).expect("parse governance.json");
        let run_id = gov_json["sections"]["trajectory"]["run_id"]
            .as_str()
            .expect("trajectory run_id in governance.json")
            .to_string();

        if !plan_configured {
            let plan_upd = self.decapod(&[
                "govern",
                "plan",
                "update",
                "--title",
                "Exact candidate validation plan",
                "--intent",
                commit_msg,
                "--todo-id",
                &self.task_id,
                "--proof-hook",
                "decapod validate",
            ]);
            assert!(
                plan_upd.success(),
                "plan update failed:\n{}",
                plan_upd.combined
            );
            let plan_approve = self.decapod(&["govern", "plan", "approve"]);
            assert!(
                plan_approve.success(),
                "plan approve failed:\n{}",
                plan_approve.combined
            );
        }

        for (spec, disposition, reason) in [
            (
                ".decapod/managed/specs/ARCHITECTURE.md",
                "unchanged-with-reason",
                "Architecture unchanged in consumer smoke candidate",
            ),
            (
                ".decapod/managed/specs/INTERFACES.md",
                "unchanged-with-reason",
                "Interfaces unchanged in consumer smoke candidate",
            ),
            (
                ".decapod/managed/specs/SECURITY.md",
                "unchanged-with-reason",
                "Security boundaries unchanged in consumer smoke candidate",
            ),
        ] {
            let rev = self.decapod(&[
                "govern",
                "plan",
                "review-spec",
                "--path",
                spec,
                "--disposition",
                disposition,
                "--reason",
                reason,
            ]);
            assert!(
                rev.success(),
                "plan review-spec {spec} failed:\n{}",
                rev.combined
            );
        }

        let traj_rec = self.decapod(&[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--modified-file",
            "src/app.txt",
            "--modified-file",
            ".decapod/managed/specs/OPERATIONS.md",
            "--check",
            "smoke=passed",
        ]);
        assert!(
            traj_rec.success(),
            "trajectory record failed:\n{}",
            traj_rec.combined
        );

        git(
            &self.worktree,
            &["add", "src/app.txt", ".decapod/managed/specs/"],
        );

        let ckpt = self.decapod(&[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            checkpoint_id,
            "--summary",
            commit_msg,
            "--proof-ref",
            "smoke=passed",
        ]);
        assert!(ckpt.success(), "checkpoint failed:\n{}", ckpt.combined);

        git(&self.worktree, &["add", "."]);
        git(&self.worktree, &["commit", "-m", commit_msg]);
        git(&self.worktree, &["rev-parse", "HEAD"])
    }

    fn assert_full_validate_ok(&self, expected_head: &str, expected_base: &str) -> (Value, Value) {
        let validate = self.decapod(&["validate", "--format", "json"]);
        assert!(
            validate.success(),
            "expected full exact-candidate validate to succeed:\n{}",
            validate.combined
        );
        let val_json = validate.json();
        assert_eq!(val_json["status"], "ok", "{val_json}");
        let expected_scope = if self.origin_bare.is_some() {
            repository_profile::READINESS_SCOPE
        } else {
            repository_profile::LOCAL_GIT_READINESS_SCOPE
        };
        assert_eq!(val_json["readiness"]["ready"], true, "{val_json}");
        assert_eq!(val_json["readiness"]["proof_ready"], true, "{val_json}");
        assert_eq!(val_json["readiness"]["scope"], expected_scope, "{val_json}");
        if self.origin_bare.is_none() {
            assert_eq!(
                val_json["publication_ready"], false,
                "canonical local target must never claim remote PR publication_ready=true: {val_json}"
            );
        }

        let evidence_dir = PathBuf::from(
            val_json["readiness"]["evidence_path"]
                .as_str()
                .expect("evidence_path"),
        );
        let proof_bytes = fs::read(evidence_dir.join("historical-proof.json"))
            .expect("read historical-proof.json");
        let proof_json: Value =
            serde_json::from_slice(&proof_bytes).expect("parse historical proof");
        assert_eq!(
            proof_json["material"]["head_revision"], expected_head,
            "{proof_json}"
        );
        assert!(
            proof_json["material"]["tree_digest"]
                .as_str()
                .is_some_and(|s| s.starts_with("sha256:") && s.len() == 71),
            "expected sha256 tree_digest in {proof_json}"
        );
        assert_eq!(
            proof_json["material"]["accepted_base_revision"], expected_base,
            "{proof_json}"
        );
        assert_eq!(
            proof_json["material"]["base_branch"], "main",
            "{proof_json}"
        );
        assert_eq!(
            proof_json["material"]["work_branch"], self.branch,
            "{proof_json}"
        );

        let receipt_bytes = fs::read(evidence_dir.join("completion/native-readiness-receipt.json"))
            .expect("read native-readiness-receipt.json");
        let receipt_json: Value = serde_json::from_slice(&receipt_bytes).expect("parse receipt");
        assert_eq!(receipt_json["material"]["head_revision"], expected_head);
        assert_eq!(
            receipt_json["material"]["tree_digest"],
            proof_json["material"]["tree_digest"]
        );

        (val_json, proof_json)
    }

    fn assert_qa_passed(&self, commit: &str, expected_base: &str, expected_gen: u64) -> Value {
        let qa = self.decapod(&["qa", "verify", "commit", commit, "--json"]);
        assert!(
            qa.success(),
            "qa verify commit {commit} failed:\n{}",
            qa.combined
        );
        let qa_json = qa.json();
        assert_eq!(qa_json["schema"], "decapod.validation-history.lookup.v1");
        assert_eq!(qa_json["scope"], "historical_validation");
        assert_eq!(qa_json["status"], "passed");
        assert_eq!(qa_json["publication_ready"], false);
        assert_eq!(qa_json["candidate_sha"], commit);
        assert_eq!(qa_json["accepted_base"], expected_base);
        assert_eq!(qa_json["task"]["task_id"], self.task_id);
        assert_eq!(qa_json["task"]["lease_generation"], expected_gen);
        qa_json
    }

    fn assert_qa_and_done_reject(&self, commit: &str, context: &str) {
        let qa = self.decapod(&["qa", "verify", "commit", commit, "--json"]);
        assert!(
            !qa.success(),
            "qa verify commit {commit} must fail after {context}, got:\n{}",
            qa.combined
        );
        let done = self.decapod_in(
            &self.repo_root,
            &[
                "todo",
                "done",
                "--id",
                &self.task_id,
                "--validated",
                "--commit",
                commit,
                "--format",
                "json",
            ],
        );
        assert!(
            !done.success(),
            "todo done --validated --commit {commit} must fail after {context}, got:\n{}",
            done.combined
        );
    }
}

#[test]
fn exact_candidate_binding_and_invalidations_on_edit_amend_rebase_and_target_advance() {
    let h = ExactCandidateHarness::new(true);
    let c1 = h.prepare_governed_worktree_commit(
        "module-v2\n",
        "c1",
        "ckpt-exact-1",
        "feat: candidate C1",
        false,
    );
    let (_, proof1) = h.assert_full_validate_ok(&c1, &h.base_sha);
    let tree1_digest = proof1["material"]["tree_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let git_tree1 = git(&h.worktree, &["rev-parse", "HEAD^{tree}"]);
    h.assert_qa_passed(&c1, &h.base_sha, 1);

    // 1. Uncommitted source edit must immediately break validate without modifying C1's record.
    fs::write(h.worktree.join("src/app.txt"), "module-dirty-uncommitted\n").unwrap();
    let dirty_val = h.decapod(&["validate", "--format", "json"]);
    assert!(
        !dirty_val.success(),
        "dirty working tree edit must fail validate:\n{}",
        dirty_val.combined
    );
    git(&h.worktree, &["checkout", "--", "src/app.txt"]);

    // 2. Tree-preserving amend creates new commit SHA C1_amended with identical git tree.
    git(
        &h.worktree,
        &[
            "commit",
            "--amend",
            "-m",
            "feat: candidate C1 amended keeping identical tree",
        ],
    );
    let c1_amended = git(&h.worktree, &["rev-parse", "HEAD"]);
    assert_ne!(c1_amended, c1, "amend must produce a distinct commit SHA");
    assert_eq!(
        git(&h.worktree, &["rev-parse", "HEAD^{tree}"]),
        git_tree1,
        "amend test assumes identical git tree"
    );
    h.assert_qa_and_done_reject(&c1_amended, "tree-preserving git commit --amend");
    // Original C1 observation stays truthful and immutable, never granting readiness to C1_amended.
    h.assert_qa_passed(&c1, &h.base_sha, 1);

    // 3. New source edit + commit C2 changes tree and invalidates prior proof.
    let c2 = h.prepare_governed_worktree_commit(
        "module-v3\n",
        "c2",
        "ckpt-exact-2",
        "feat: candidate C2",
        true,
    );
    assert_ne!(c2, c1);
    assert_ne!(c2, c1_amended);
    h.assert_qa_and_done_reject(&c2, "source edit and new commit C2");
    h.assert_qa_passed(&c1, &h.base_sha, 1);

    // Re-validate C2 cleanly -> proof binds C2 and a new tree_digest.
    let (_, proof2) = h.assert_full_validate_ok(&c2, &h.base_sha);
    assert_ne!(
        proof2["material"]["tree_digest"].as_str().unwrap(),
        tree1_digest,
        "tree_digest must change when source contents change"
    );
    h.assert_qa_passed(&c2, &h.base_sha, 1);

    // 4. Advance accepted base branch (main: B0 -> B1).
    fs::write(h.repo_root.join("notes.txt"), "upstream accepted change\n").unwrap();
    git(&h.repo_root, &["add", "notes.txt"]);
    git(
        &h.repo_root,
        &["commit", "-m", "chore: advance accepted main"],
    );
    let b1 = git(&h.repo_root, &["rev-parse", "HEAD"]);
    assert_ne!(b1, h.base_sha);

    // Before rebase/integration: C2 was never merged into main and accepted_base moved B0 -> B1,
    // so todo done --validated --commit C2 must refuse completion on stale accepted base.
    let done_stale_base = h.decapod_in(
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &h.task_id,
            "--validated",
            "--commit",
            &c2,
            "--format",
            "json",
        ],
    );
    assert!(
        !done_stale_base.success(),
        "todo done must reject unintegrated candidate after main advances:\n{}",
        done_stale_base.combined
    );
    // C2's historical qa record stays bound to original B0 without forging readiness against B1.
    h.assert_qa_passed(&c2, &h.base_sha, 1);

    // 5. Rebase C2 onto B1 -> produces new commit SHA C2_rebased with no validation proof.
    git(&h.worktree, &["rebase", "main"]);
    let c2_rebased = git(&h.worktree, &["rev-parse", "HEAD"]);
    assert_ne!(c2_rebased, c2);
    h.assert_qa_and_done_reject(&c2_rebased, "rebase onto advanced main");

    // Rebased commit without a fresh material checkpoint must be rejected immediately.
    let unckpt_val = h.decapod(&["validate", "--format", "json"]);
    assert!(
        !unckpt_val.success(),
        "un-checkpointed rebased commit must fail validate:\n{}",
        unckpt_val.combined
    );
    assert!(
        unckpt_val
            .combined
            .contains("GOVERNANCE_CHECKPOINT_MISSING"),
        "expected GOVERNANCE_CHECKPOINT_MISSING on rebased commit, got:\n{}",
        unckpt_val.combined
    );

    // Squash/re-checkpoint cleanly onto B1 so every commit in main..HEAD carries a valid
    // exact-material checkpoint, then re-validate against B1.
    git(&h.worktree, &["reset", "--soft", &b1]);
    let refresh_rebased = h.decapod(&["rpc", "--op", "specs.refresh"]);
    assert!(
        refresh_rebased.success(),
        "specs.refresh after rebase failed:\n{}",
        refresh_rebased.combined
    );
    git(
        &h.worktree,
        &["add", "src/app.txt", ".decapod/managed/specs/"],
    );
    let ckpt_rebased = h.decapod(&[
        "govern",
        "artifacts",
        "checkpoint",
        "--id",
        "ckpt-rebased-1",
        "--summary",
        "feat: rebased candidate onto B1",
        "--proof-ref",
        "smoke=passed",
    ]);
    assert!(
        ckpt_rebased.success(),
        "rebased checkpoint failed:\n{}",
        ckpt_rebased.combined
    );
    git(&h.worktree, &["add", ".decapod/governance.json"]);
    git(
        &h.worktree,
        &["commit", "-m", "feat: rebased candidate onto B1"],
    );
    let c2_rebased_ckpt = git(&h.worktree, &["rev-parse", "HEAD"]);

    h.assert_full_validate_ok(&c2_rebased_ckpt, &b1);
    h.assert_qa_passed(&c2_rebased_ckpt, &b1, 1);
    git(&h.repo_root, &["merge", "--ff-only", &c2_rebased_ckpt]);
    let done_ok = h.decapod_in(
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &h.task_id,
            "--validated",
            "--commit",
            &c2_rebased_ckpt,
            "--format",
            "json",
        ],
    );
    assert!(
        done_ok.success(),
        "todo done after integrating freshly validated rebased candidate failed:\n{}",
        done_ok.combined
    );
}

#[test]
fn task_lease_renewal_and_workunit_custody_invalidate_stale_proof() {
    let h = ExactCandidateHarness::new(true);
    let gov_raw = fs::read_to_string(h.worktree.join(".decapod/governance.json"))
        .expect("read governance.json");
    let gov_json: Value = serde_json::from_str(&gov_raw).expect("parse governance.json");
    let intent_id = gov_json["sections"]["trajectory"]["intent_id"]
        .as_str()
        .expect("intent_id in governance.json");

    // Capture deterministic context capsule & initialize workunit bound to the authoritative intent.
    let capsule = h.decapod(&[
        "govern",
        "capsule",
        "query",
        "--topic",
        "exact-candidate",
        "--scope",
        "interfaces",
        "--task-id",
        &h.task_id,
        "--write",
    ]);
    assert!(
        capsule.success(),
        "capsule query --write failed:\n{}",
        capsule.combined
    );
    let capsule_rel = format!(".decapod/managed/context/{}.json", h.task_id);

    let wu_init = h.decapod(&[
        "govern",
        "workunit",
        "init",
        "--task-id",
        &h.task_id,
        "--intent-ref",
        intent_id,
    ]);
    assert!(
        wu_init.success(),
        "workunit init failed:\n{}",
        wu_init.combined
    );
    let wu_spec = h.decapod(&[
        "govern",
        "workunit",
        "attach-spec",
        "--task-id",
        &h.task_id,
        "--ref",
        ".decapod/managed/specs/INTENT.md",
    ]);
    assert!(
        wu_spec.success(),
        "workunit attach-spec failed:\n{}",
        wu_spec.combined
    );
    let wu_state = h.decapod(&[
        "govern",
        "workunit",
        "attach-state",
        "--task-id",
        &h.task_id,
        "--ref",
        &capsule_rel,
    ]);
    assert!(
        wu_state.success(),
        "workunit attach-state failed:\n{}",
        wu_state.combined
    );
    let wu_plan = h.decapod(&[
        "govern",
        "workunit",
        "set-proof-plan",
        "--task-id",
        &h.task_id,
        "--gate",
        "validate_passes",
    ]);
    assert!(
        wu_plan.success(),
        "workunit set-proof-plan failed:\n{}",
        wu_plan.combined
    );
    for to in ["executing", "claimed"] {
        let step = h.decapod(&[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &h.task_id,
            "--to",
            to,
        ]);
        assert!(
            step.success(),
            "workunit transition --to {to} failed:\n{}",
            step.combined
        );
    }

    let c1 = h.prepare_governed_worktree_commit(
        "module-lease-v1\n",
        "lease-c1",
        "ckpt-lease-1",
        "feat: lease generation 1 candidate",
        false,
    );
    let wu_ckpt = h.decapod(&[
        "govern",
        "workunit",
        "checkpoint",
        "--task-id",
        &h.task_id,
        "--operation-id",
        "op-lease-gen-1",
    ]);
    assert!(
        wu_ckpt.success(),
        "workunit checkpoint failed:\n{}",
        wu_ckpt.combined
    );
    let smoke_artifact = ".decapod/data/smoke-proof-gen1.txt";
    fs::write(h.worktree.join(smoke_artifact), "smoke pass gen1\n").unwrap();
    let wu_rec1 = h.decapod(&[
        "govern",
        "workunit",
        "record-proof",
        "--task-id",
        &h.task_id,
        "--gate",
        "validate_passes",
        "--status",
        "pass",
        "--artifact",
        smoke_artifact,
    ]);
    assert!(
        wu_rec1.success(),
        "workunit record-proof gen1 failed:\n{}",
        wu_rec1.combined
    );

    h.assert_full_validate_ok(&c1, &h.base_sha);
    h.assert_qa_passed(&c1, &h.base_sha, 1);

    // Renew lease -> bumps lease_generation from 1 to 2.
    let renew = h.decapod_in(
        &h.repo_root,
        &["todo", "renew", "--id", &h.task_id, "--format", "json"],
    );
    assert!(renew.success(), "todo renew failed:\n{}", renew.combined);
    let renew_json = renew.json();
    let new_gen = renew_json["lease_generation"]
        .as_u64()
        .or_else(|| renew_json["result"]["lease_generation"].as_u64())
        .expect("lease_generation from todo renew");
    assert_eq!(new_gen, 2, "{renew_json}");

    // Even after merging C1 into main, stale gen-1 validation history must NOT complete gen-2 task.
    git(&h.repo_root, &["merge", "--ff-only", &c1]);
    let done_stale_lease = h.decapod_in(
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &h.task_id,
            "--validated",
            "--commit",
            &c1,
            "--format",
            "json",
        ],
    );
    assert!(
        !done_stale_lease.success(),
        "todo done must reject gen-1 proof after lease_generation bumped to 2:\n{}",
        done_stale_lease.combined
    );
    assert!(
        done_stale_lease
            .combined
            .contains("VALIDATION_HISTORY_TASK_STALE")
            || done_stale_lease.combined.contains("lease_generation"),
        "expected custody staleness error, got:\n{}",
        done_stale_lease.combined
    );

    // Workunit verified transition and resume must reject stale gen-1 checkpoint/proof.
    let wu_verify_stale = h.decapod(&[
        "govern",
        "workunit",
        "transition",
        "--task-id",
        &h.task_id,
        "--to",
        "verified",
    ]);
    assert!(
        !wu_verify_stale.success(),
        "workunit transition --to verified must reject after lease renewal:\n{}",
        wu_verify_stale.combined
    );
    let wu_resume_stale = h.decapod(&[
        "govern",
        "workunit",
        "resume",
        "--task-id",
        &h.task_id,
        "--operation-id",
        "op-resume-after-renew",
    ]);
    assert!(
        !wu_resume_stale.success(),
        "workunit resume must fail after lease_generation bumped:\n{}",
        wu_resume_stale.combined
    );

    // Historical qa verify commit <C1> still truthfully shows C1's original gen-1 record.
    h.assert_qa_passed(&c1, &h.base_sha, 1);

    // Create a second task on the same repo, renew its lease to generation 2, and prove
    // a freshly validated candidate C2 binds lease_generation=2 and completes cleanly.
    let todo2_add = h.decapod_in(
        &h.repo_root,
        &[
            "todo",
            "add",
            "Exact candidate gen-2 completion task",
            "--format",
            "json",
        ],
    );
    assert!(
        todo2_add.success(),
        "todo2 add failed:\n{}",
        todo2_add.combined
    );
    let todo2_json = todo2_add.json();
    let task2_id = todo2_json["id"]
        .as_str()
        .or_else(|| todo2_json["result"]["id"].as_str())
        .expect("task2 id")
        .to_string();
    let todo2_claim = h.decapod_in(
        &h.repo_root,
        &["todo", "claim", "--id", &task2_id, "--format", "json"],
    );
    assert!(
        todo2_claim.success(),
        "todo2 claim failed:\n{}",
        todo2_claim.combined
    );
    let renew2 = h.decapod_in(
        &h.repo_root,
        &["todo", "renew", "--id", &task2_id, "--format", "json"],
    );
    assert!(renew2.success(), "todo2 renew failed:\n{}", renew2.combined);
    let renew2_json = renew2.json();
    let gen2 = renew2_json["lease_generation"]
        .as_u64()
        .or_else(|| renew2_json["result"]["lease_generation"].as_u64())
        .expect("task2 lease_generation");
    assert_eq!(gen2, 2, "{renew2_json}");

    let branch2 = format!("agent/{AGENT_ID}/{task2_id}-gen2");
    let ensure2 = h.decapod_in(&h.repo_root, &["workspace", "ensure", "--branch", &branch2]);
    assert!(
        ensure2.success(),
        "workspace ensure2 failed:\n{}",
        ensure2.combined
    );
    let wt2 = PathBuf::from(
        ensure2.json()["worktree_path"]
            .as_str()
            .expect("worktree2 path"),
    );

    fs::write(wt2.join("src/app.txt"), "module-lease-v2\n").unwrap();
    update_operations_spec(&wt2, "lease-c2");
    assert!(
        h.decapod_in(&wt2, &["rpc", "--op", "specs.refresh"])
            .success()
    );
    let gov2: Value =
        serde_json::from_str(&fs::read_to_string(wt2.join(".decapod/governance.json")).unwrap())
            .unwrap();
    let run2_id = gov2["sections"]["trajectory"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        h.decapod_in(
            &wt2,
            &[
                "govern",
                "plan",
                "update",
                "--title",
                "Exact candidate gen-2 plan",
                "--intent",
                "feat: fresh candidate C2 under lease generation 2",
                "--todo-id",
                &task2_id,
                "--proof-hook",
                "decapod validate",
            ],
        )
        .success()
    );
    assert!(h.decapod_in(&wt2, &["govern", "plan", "approve"]).success());
    for (spec, reason) in [
        (
            ".decapod/managed/specs/ARCHITECTURE.md",
            "Architecture unchanged in gen-2 check",
        ),
        (
            ".decapod/managed/specs/INTERFACES.md",
            "Interfaces unchanged in gen-2 check",
        ),
        (
            ".decapod/managed/specs/SECURITY.md",
            "Security boundaries unchanged in gen-2 check",
        ),
    ] {
        assert!(
            h.decapod_in(
                &wt2,
                &[
                    "govern",
                    "plan",
                    "review-spec",
                    "--path",
                    spec,
                    "--disposition",
                    "unchanged-with-reason",
                    "--reason",
                    reason,
                ],
            )
            .success()
        );
    }
    assert!(
        h.decapod_in(
            &wt2,
            &[
                "govern",
                "trajectory",
                "record",
                "--run-id",
                &run2_id,
                "--modified-file",
                "src/app.txt",
                "--modified-file",
                ".decapod/managed/specs/OPERATIONS.md",
                "--check",
                "smoke=passed",
            ],
        )
        .success()
    );
    git(&wt2, &["add", "src/app.txt", ".decapod/managed/specs/"]);
    assert!(
        h.decapod_in(
            &wt2,
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                "ckpt-lease-2",
                "--summary",
                "feat: fresh candidate C2 under lease generation 2",
                "--proof-ref",
                "smoke=passed",
            ],
        )
        .success()
    );
    git(&wt2, &["add", "."]);
    git(
        &wt2,
        &[
            "commit",
            "-m",
            "feat: fresh candidate C2 under lease generation 2",
        ],
    );
    let c2 = git(&wt2, &["rev-parse", "HEAD"]);

    let val2 = h.decapod_in(&wt2, &["validate", "--format", "json"]);
    assert!(
        val2.success(),
        "validate C2 under gen-2 failed:\n{}",
        val2.combined
    );
    let qa2 = h.decapod_in(&wt2, &["qa", "verify", "commit", &c2, "--json"]);
    assert!(
        qa2.success(),
        "qa verify commit C2 failed:\n{}",
        qa2.combined
    );
    let qa2_json = qa2.json();
    assert_eq!(qa2_json["task"]["task_id"], task2_id);
    assert_eq!(qa2_json["task"]["lease_generation"], 2);
    assert_eq!(qa2_json["accepted_base"], c1);

    git(&h.repo_root, &["merge", "--ff-only", &c2]);
    let done_gen2 = h.decapod_in(
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &task2_id,
            "--validated",
            "--commit",
            &c2,
            "--format",
            "json",
        ],
    );
    assert!(
        done_gen2.success(),
        "todo done after fresh gen-2 validation on C2 failed:\n{}",
        done_gen2.combined
    );
}

#[test]
fn ci_local_project_check_and_publish_freshness_boundaries() {
    // 1. Canonical local repo exercises ci_local, rpc validate.run, and govern project-check
    //    partial scopes, verifying none grant completion/publication authority, then proves
    //    real full validation succeeds (with publication_ready=false) while workspace publish
    //    remains explicitly blocked.
    let h = ExactCandidateHarness::new(true);
    let c1 = h.prepare_governed_worktree_commit(
        "module-ci-boundary\n",
        "ci-boundary",
        "ckpt-ci-1",
        "feat: candidate C1 for scope boundary",
        false,
    );

    // GITHUB_ACTIONS=true -> ci_local, publication_ready=false, readiness=null
    let gh_val = h.decapod_with_env(
        &["validate", "--format", "json"],
        &[("GITHUB_ACTIONS", "true")],
    );
    assert!(
        gh_val.success(),
        "GITHUB_ACTIONS ci_local validate failed:\n{}",
        gh_val.combined
    );
    let gh_json = gh_val.json();
    assert_eq!(gh_json["scope"], "ci_local", "{gh_json}");
    assert_eq!(gh_json["publication_ready"], false, "{gh_json}");
    assert!(gh_json["readiness"].is_null(), "{gh_json}");

    // BUILDKITE=true -> ci_local, publication_ready=false, readiness=null
    let bk_val = h.decapod_with_env(&["validate", "--format", "json"], &[("BUILDKITE", "true")]);
    assert!(
        bk_val.success(),
        "BUILDKITE ci_local validate failed:\n{}",
        bk_val.combined
    );
    let bk_json = bk_val.json();
    assert_eq!(bk_json["scope"], "ci_local", "{bk_json}");
    assert_eq!(bk_json["publication_ready"], false, "{bk_json}");
    assert!(bk_json["readiness"].is_null(), "{bk_json}");

    // RPC validate.run under GITHUB_ACTIONS=true must also stay ci_local and non-ready
    let rpc_ci = h.decapod_with_env(
        &["rpc", "--op", "validate.run"],
        &[("GITHUB_ACTIONS", "true")],
    );
    assert!(
        rpc_ci.success(),
        "GITHUB_ACTIONS rpc validate.run failed:\n{}",
        rpc_ci.combined
    );
    let rpc_json = rpc_ci.json();
    assert_eq!(rpc_json["result"]["scope"], "ci_local", "{rpc_json}");
    assert_eq!(rpc_json["result"]["publication_ready"], false, "{rpc_json}");

    // govern project-check succeeds only for bounded prerequisites, never publication readiness
    let proj_chk = h.decapod(&["govern", "project-check", "--format", "json"]);
    assert!(
        proj_chk.success(),
        "govern project-check failed:\n{}",
        proj_chk.combined
    );
    let proj_json = proj_chk.json();
    assert_eq!(proj_json["publication_ready"], false, "{proj_json}");

    // None of ci_local, rpc ci_local, or govern project-check may mint historical readiness or done authority.
    h.assert_qa_and_done_reject(&c1, "ci_local and project-check partial scopes");

    // workspace publish must refuse publication when only ci_local/project-check ran
    let pub_before = h.decapod(&[
        "workspace",
        "publish",
        "--title",
        "Should be blocked",
        "--description",
        "No full validation yet",
    ]);
    assert!(
        !pub_before.success(),
        "workspace publish must fail with only ci_local/project-check:\n{}",
        pub_before.combined
    );

    // Restore clean committed governance.json (project-check writes a non-readiness receipt section)
    // before executing full exact-candidate validation on C1.
    git(
        &h.worktree,
        &["checkout", "HEAD", "--", ".decapod/governance.json"],
    );
    let status_clean = git(&h.worktree, &["status", "--porcelain"]);
    assert!(
        status_clean.is_empty(),
        "expected clean worktree after checkout governance.json, got:\n{status_clean}"
    );

    // Full local exact-candidate validation succeeds with publication_ready=false,
    // and canonical_local_target explicitly forbids forge workspace publish.
    h.assert_full_validate_ok(&c1, &h.base_sha);
    h.assert_qa_passed(&c1, &h.base_sha, 1);
    let local_pub = h.decapod(&[
        "workspace",
        "publish",
        "--title",
        "Local target cannot publish",
        "--description",
        "Local readiness is never forge readiness",
    ]);
    assert!(
        !local_pub.success(),
        "workspace publish must reject canonical_local_target:\n{}",
        local_pub.combined
    );
    assert!(
        local_pub
            .combined
            .contains("LOCAL_TARGET_PUBLICATION_UNSUPPORTED"),
        "expected LOCAL_TARGET_PUBLICATION_UNSUPPORTED, got:\n{}",
        local_pub.combined
    );
}

#[test]
fn remote_unvalidated_publish_and_snapshot_prepare_refuse_without_full_proof() {
    // Non-canonical remote target (canonical_local_target = false, pushed to bare origin)
    // must refuse workspace publish and snapshot-prepare when only ci_local ran.
    let remote_h = ExactCandidateHarness::new(false);
    let remote_c1 = remote_h.prepare_governed_worktree_commit(
        "module-remote-boundary\n",
        "remote-boundary",
        "ckpt-remote-1",
        "feat: remote target boundary check",
        false,
    );
    git(
        &remote_h.worktree,
        &["push", "-u", "origin", &remote_h.branch],
    );
    let remote_ci = remote_h.decapod_with_env(
        &["validate", "--format", "json"],
        &[("GITHUB_ACTIONS", "true")],
    );
    assert!(
        remote_ci.success(),
        "remote ci_local failed:\n{}",
        remote_ci.combined
    );
    assert_eq!(remote_ci.json()["scope"], "ci_local");
    assert_eq!(remote_ci.json()["publication_ready"], false);
    remote_h.assert_qa_and_done_reject(&remote_c1, "remote ci_local only");

    let remote_pub = remote_h.decapod(&[
        "workspace",
        "publish",
        "--title",
        "Remote unvalidated publish blocked",
        "--description",
        "Only ci_local ran",
    ]);
    assert!(
        !remote_pub.success(),
        "workspace publish must reject unvalidated remote branch:\n{}",
        remote_pub.combined
    );
    let prep = remote_h.decapod(&["validate", "--snapshot-prepare"]);
    assert!(
        !prep.success(),
        "snapshot-prepare must never return success without completed remote proof:\n{}",
        prep.combined
    );
}
