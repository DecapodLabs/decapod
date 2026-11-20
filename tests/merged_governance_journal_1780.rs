use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const ACTOR: &str = "tester";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_raw(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn cli(dir: &Path, actor: &str, password: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    for (k, _) in std::env::vars_os() {
        let s = k.to_string_lossy();
        if s.starts_with("DECAPOD_") || s.starts_with("GIT_") {
            cmd.env_remove(k);
        }
    }
    cmd.current_dir(dir)
        .env("DECAPOD_AGENT_ID", actor)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_STORAGE_OWNER", "1");
    if let Some(pw) = password {
        cmd.env("DECAPOD_SESSION_PASSWORD", pw);
    }
    cmd.args(args).output().unwrap()
}

fn success_json(out: Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "command failed ({:?}):\nstdout: {stdout}\nstderr: {stderr}",
        out.status.code()
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("expected JSON stdout ({e}):\nstdout: {stdout}\nstderr: {stderr}")
    })
}

fn failure_text(out: Output) -> String {
    assert!(
        !out.status.success(),
        "expected failure, got success:\nstdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

struct TestRepo {
    _root: TempDir,
    root_path: PathBuf,
    password: String,
    base_commit: String,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        git(&root, &["init", "-b", "master"]);
        git(&root, &["config", "user.name", "Test User"]);
        git(&root, &["config", "user.email", "test@example.invalid"]);
        fs::write(root.join("README.md"), "# test\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "-m", "initial"]);

        let init_out = cli(
            &root,
            ACTOR,
            None,
            &[
                "init",
                "--force",
                "--no-ci",
                "--no-specs",
                "--no-container-workspaces",
            ],
        );
        assert!(
            init_out.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init_out.stderr)
        );
        // Ignore runtime sqlite & lock files in git
        let existing_gi = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
        if !existing_gi.contains(".decapod/data/") {
            fs::write(
                root.join(".gitignore"),
                format!(
                    "{existing_gi}\n.decapod/data/\n.decapod/managed/governance-document.lock\n"
                ),
            )
            .unwrap();
        }
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-m", "chore: decapod init"]);

        let session_out = cli(&root, ACTOR, None, &["session", "acquire"]);
        assert!(
            session_out.status.success(),
            "session acquire failed: {} {}",
            String::from_utf8_lossy(&session_out.stdout),
            String::from_utf8_lossy(&session_out.stderr)
        );
        let session_text = String::from_utf8_lossy(&session_out.stdout);
        let password = session_text
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("Password:")
                    .map(|p| p.trim().to_string())
            })
            .unwrap_or_else(|| {
                let parsed: Value = serde_json::from_str(&session_text).unwrap();
                parsed["password"].as_str().unwrap().to_string()
            });

        let base_commit = git(&root, &["rev-parse", "HEAD"]);
        Self {
            _root: dir,
            root_path: root,
            password,
            base_commit,
        }
    }

    fn add_and_claim_todo(&self, title: &str) -> String {
        let added = success_json(cli(
            &self.root_path,
            ACTOR,
            Some(&self.password),
            &["todo", "add", title],
        ));
        let id = added["id"].as_str().unwrap().to_string();
        success_json(cli(
            &self.root_path,
            ACTOR,
            Some(&self.password),
            &["todo", "claim", "--id", &id, "--lease-seconds", "3600"],
        ));
        id
    }

    fn ensure_worktree(&self, branch: &str) -> PathBuf {
        let out = success_json(cli(
            &self.root_path,
            ACTOR,
            Some(&self.password),
            &["workspace", "ensure", "--branch", branch],
        ));
        PathBuf::from(out["worktree_path"].as_str().unwrap())
    }
}

fn commit_checkpoint(wt: &Path, pw: &str, ckpt_id: &str, msg: &str) -> String {
    git(wt, &["add", "-A"]);
    success_json(cli(
        wt,
        ACTOR,
        Some(pw),
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            ckpt_id,
            "--summary",
            msg,
        ],
    ));
    git(wt, &["add", ".decapod/governance.json"]);
    git(wt, &["commit", "-m", msg]);
    git(wt, &["rev-parse", "HEAD"])
}

fn setup_shared_workstream(
    repo: &TestRepo,
    _task_id: &str,
    branch: &str,
) -> (PathBuf, String, String) {
    let wt = repo.ensure_worktree(branch);
    git(&wt, &["config", "user.name", "Test User"]);
    git(&wt, &["config", "user.email", "test@example.invalid"]);

    let initial_doc = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "status"],
    ));
    let change_id = initial_doc["change"]["id"].as_str().unwrap().to_string();
    let run_id = initial_doc["sections"]["trajectory"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();

    fs::create_dir_all(wt.join("src")).unwrap();
    fs::write(wt.join("src/common.rs"), "pub fn base() -> u32 { 1 }\n").unwrap();
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--inspected-file",
            "src/common.rs",
            "--modified-file",
            "src/common.rs",
            "--command",
            "cargo check",
            "--tool-call",
            "run_command",
            "--check",
            "baseline=passed",
            "--evidence",
            "base:ok",
        ],
    ));
    commit_checkpoint(&wt, &repo.password, "ckpt-shared-base", "shared base");
    (wt, change_id, run_id)
}

#[test]
fn cli_reconciles_two_branch_no_commit_ours_merge_hash_mismatch_and_verifies_checkpoints() {
    let repo = TestRepo::new();
    let task_id = repo.add_and_claim_todo("Shared merge recovery #1780");
    let main_branch = format!("agent/{ACTOR}/{task_id}-main-wt");
    let (wt, change_id, run_id) = setup_shared_workstream(&repo, &task_id, &main_branch);
    let shared_sha = git(&wt, &["rev-parse", "HEAD"]);
    let intent_id = {
        let doc = success_json(cli(
            &wt,
            ACTOR,
            Some(&repo.password),
            &["govern", "artifacts", "status"],
        ));
        doc["sections"]["trajectory"]["intent_id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // Branch B modifies an upper trajectory field (`inspected_files` / `modified_files`)
    // separated from `artifact_hash` by unchanged `declared_commands`/`tool_calls`/`checks`
    // lines so `git merge --no-commit -X ours` mixes fields -> `Trajectory artifact hash mismatch`.
    fs::write(wt.join("src/b.rs"), "pub fn b() -> u32 { 3 }\n").unwrap();
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--inspected-file",
            "src/b.rs",
            "--modified-file",
            "src/b.rs",
        ],
    ));
    let side_sha = commit_checkpoint(&wt, &repo.password, "ckpt-branch-b", "branch b work");
    git(&wt, &["branch", "side-branch-b", &side_sha]);
    git(&wt, &["reset", "--hard", &shared_sha]);

    // Branch A modifies lower fields (`checks`, `loops`, `evidence`) and `src/a.rs`
    fs::write(wt.join("src/a.rs"), "pub fn a() -> u32 { 2 }\n").unwrap();
    let loop_a = json!({
        "intent_id": intent_id,
        "trajectory_id": run_id,
        "loop_id": "verify-a",
        "loop_type": "verification",
        "attempt": 1,
        "trigger": "agent",
        "grader_result": "pass",
        "feedback": "a passed",
        "proof_refs": ["test:a"],
        "mutation_proposal": "none",
        "status": "passed"
    })
    .to_string();
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--check",
            "unit_a=passed",
            "--evidence",
            "a:passed",
            "--loop-json",
            &loop_a,
        ],
    ));
    commit_checkpoint(&wt, &repo.password, "ckpt-branch-a", "branch a work");

    // Reproduce #1780: `git merge --no-commit -X ours` mixes trajectory fields/hashes
    let merge_out = git_raw(&wt, &["merge", "--no-commit", "-X", "ours", &side_sha]);
    assert!(
        merge_out.status.success(),
        "merge -X ours failed: {}",
        String::from_utf8_lossy(&merge_out.stderr)
    );

    let broken_traj = failure_text(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "trajectory", "status", "--run-id", &run_id],
    ));
    assert!(
        broken_traj.contains("Trajectory artifact hash mismatch"),
        "expected Trajectory artifact hash mismatch before reconciliation, got: {broken_traj}"
    );
    let broken_artifacts = failure_text(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "status"],
    ));
    assert!(
        broken_artifacts.contains("Trajectory artifact hash mismatch"),
        "expected artifacts status to fail on hash mismatch before reconciliation, got: {broken_artifacts}"
    );

    // Supported recovery via `decapod govern artifacts reconcile-merge`
    let reconciled = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "reconcile-merge"],
    ));
    assert_eq!(reconciled["change"]["id"], change_id);
    let ckpt_ids: Vec<&str> = reconciled["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ckpt_ids,
        vec!["ckpt-shared-base", "ckpt-branch-a", "ckpt-branch-b"]
    );

    // Both status surfaces must now pass without manual hash editing
    let artifacts_ok = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "status"],
    ));
    assert_eq!(artifacts_ok["change"]["id"], change_id);

    let traj_ok = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "trajectory", "status", "--run-id", &run_id],
    ));
    assert_eq!(traj_ok["status"], "ok", "{traj_ok:?}");
    assert_eq!(traj_ok["proof_status"], "passed", "{traj_ok:?}");

    let traj_section = &artifacts_ok["sections"]["trajectory"];
    let modified: Vec<&str> = traj_section["modified_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(modified.contains(&"src/common.rs"));
    assert!(modified.contains(&"src/b.rs"));

    let check_names: Vec<&str> = traj_section["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(check_names.contains(&"baseline"));
    assert!(check_names.contains(&"unit_a"));

    // Record merge checkpoint and commit the merge; checkpoint history must prove cleanly
    let merged_sha = commit_checkpoint(&wt, &repo.password, "ckpt-merge-1780", "merge branches");
    let report = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            &repo.base_commit,
            "--head-ref",
            &merged_sha,
            "--target-base",
            "master",
            "--format",
            "json",
        ],
    ));
    assert_eq!(report["disposition"], "proven", "{report:?}");
    assert_eq!(report["history_proven"], true, "{report:?}");
    assert_eq!(report["findings"], json!([]), "{report:?}");
}

#[test]
fn cli_reconcile_merge_never_promotes_failed_or_missing_proof_and_refuses_conflicts() {
    let repo = TestRepo::new();
    let task_id = repo.add_and_claim_todo("Negative controls #1780");
    let main_branch = format!("agent/{ACTOR}/{task_id}-neg-main");
    let (wt, _change_id, run_id) = setup_shared_workstream(&repo, &task_id, &main_branch);
    let shared_sha = git(&wt, &["rev-parse", "HEAD"]);

    // 1. Non-merge invocation must fail closed with GOVERNANCE_MERGE_NOT_IN_PROGRESS
    let non_merge_err = failure_text(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "reconcile-merge"],
    ));
    assert!(
        non_merge_err.contains("GOVERNANCE_MERGE_NOT_IN_PROGRESS"),
        "{non_merge_err}"
    );

    // 2. Merging a branch with a FAILED check must preserve failure (never become passing)
    fs::write(wt.join("src/fail.rs"), "pub fn fail() {}\n").unwrap();
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--inspected-file",
            "src/fail.rs",
            "--check",
            "security_gate=failed",
        ],
    ));
    let fail_sha = commit_checkpoint(&wt, &repo.password, "ckpt-fail", "failing check");
    git(&wt, &["branch", "neg-fail-branch", &fail_sha]);
    git(&wt, &["reset", "--hard", &shared_sha]);

    // 3a. Prepare conflicting-check branch off shared_sha (`unit_pass=failed`)
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--check",
            "unit_pass=failed",
        ],
    ));
    let conflict_sha = commit_checkpoint(
        &wt,
        &repo.password,
        "ckpt-conflict",
        "conflicting unit_pass=failed",
    );
    git(&wt, &["branch", "neg-conflict-branch", &conflict_sha]);
    git(&wt, &["reset", "--hard", &shared_sha]);

    // Advance main branch with an unrelated passing check (`unit_pass=passed`)
    fs::write(wt.join("src/pass.rs"), "pub fn pass() {}\n").unwrap();
    success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--inspected-file",
            "src/pass.rs",
            "--check",
            "unit_pass=passed",
        ],
    ));
    commit_checkpoint(&wt, &repo.password, "ckpt-pass", "passing work");

    let merge_fail = git_raw(&wt, &["merge", "--no-commit", "-X", "ours", &fail_sha]);
    assert!(merge_fail.status.success());

    let reconciled_fail = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "reconcile-merge"],
    ));
    assert!(
        reconciled_fail["sections"].get("validation").is_none(),
        "stale validation receipt must be cleared after trajectory merge"
    );
    let traj_after = success_json(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "trajectory", "status", "--run-id", &run_id],
    ));
    assert_eq!(traj_after["proof_status"], "failed", "{traj_after:?}");
    assert_eq!(
        traj_after["verdicts"]["completion_proof"], "unsupported",
        "{traj_after:?}"
    );
    git(&wt, &["merge", "--abort"]);

    // 3b. Conflicting status for the SAME check name must fail closed with GOVERNANCE_MERGE_PROOF_CONFLICT
    let _ = git_raw(&wt, &["merge", "--no-commit", "-X", "ours", &conflict_sha]);
    let proof_conflict = failure_text(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "reconcile-merge"],
    ));
    assert!(
        proof_conflict.contains("GOVERNANCE_MERGE_PROOF_CONFLICT"),
        "{proof_conflict}"
    );
    git(&wt, &["merge", "--abort"]);

    // 4. Unaccepted distinct change.id must fail closed with GOVERNANCE_MERGE_AUTHORITY_CONFLICT
    let other_task = repo.add_and_claim_todo("Competing unaccepted PR #1780");
    let other_branch = format!("agent/{ACTOR}/{other_task}-other-pr");
    let other_wt = repo.ensure_worktree(&other_branch);
    git(&other_wt, &["config", "user.name", "Test User"]);
    git(&other_wt, &["config", "user.email", "test@example.invalid"]);
    fs::create_dir_all(other_wt.join("src")).unwrap();
    fs::write(other_wt.join("src/competing.rs"), "pub fn c() {}\n").unwrap();
    let other_sha = commit_checkpoint(
        &other_wt,
        &repo.password,
        "ckpt-competing",
        "competing unaccepted PR",
    );
    let _ = git_raw(&wt, &["merge", "--no-commit", "-X", "ours", &other_sha]);
    let auth_conflict = failure_text(cli(
        &wt,
        ACTOR,
        Some(&repo.password),
        &["govern", "artifacts", "reconcile-merge"],
    ));
    assert!(
        auth_conflict.contains("GOVERNANCE_MERGE_AUTHORITY_CONFLICT"),
        "{auth_conflict}"
    );
    git(&wt, &["merge", "--abort"]);
}
