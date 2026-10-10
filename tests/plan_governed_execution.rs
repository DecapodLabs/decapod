use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .args(args)
        .output()
        .expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let repo_dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&repo_dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&repo_dir, &["init", "--force"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    Command::new("git")
        .current_dir(&repo_dir)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config user.name");
    Command::new("git")
        .current_dir(&repo_dir)
        .args(["config", "user.email", "test@example.com"])
        .output()
        .expect("git config user.email");

    let add = Command::new("git")
        .current_dir(&repo_dir)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = Command::new("git")
        .current_dir(&repo_dir)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed");

    let worktree_dir = tmp.path().join("worktree");
    let worktree = Command::new("git")
        .current_dir(&repo_dir)
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test/plan-governed",
            worktree_dir
                .to_str()
                .expect("tempdir path should be valid unicode"),
            "HEAD",
        ])
        .output()
        .expect("git worktree add");
    assert!(worktree.status.success(), "git worktree add failed");

    let add_todo = run_decapod(
        &worktree_dir,
        &["todo", "add", "Wire plan-governed execution test fixture"],
    );
    assert!(
        add_todo.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add_todo.stderr)
    );
    let todo_json: serde_json::Value =
        serde_json::from_slice(&add_todo.stdout).expect("todo add json");
    let todo_id = todo_json["id"].as_str().expect("todo id").to_string();

    (tmp, worktree_dir, todo_id)
}

#[test]
fn plan_gate_returns_needs_human_input_until_questions_cleared() {
    let (_tmp, dir, todo_id) = setup_repo();

    let init_plan = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "MVP slice",
            "--intent",
            "Enforce plan-governed execution",
            "--todo-id",
            &todo_id,
            "--question",
            "Which acceptance test should be mandatory?",
        ],
    );
    assert!(
        init_plan.status.success(),
        "plan init failed: {}",
        String::from_utf8_lossy(&init_plan.stderr)
    );

    let approve = run_decapod(&dir, &["govern", "plan", "approve"]);
    assert!(
        approve.status.success(),
        "plan approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );

    let blocked = run_decapod(
        &dir,
        &["govern", "plan", "check-execute", "--todo-id", &todo_id],
    );
    assert!(
        !blocked.status.success(),
        "check-execute should fail while human questions remain"
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains("NEEDS_HUMAN_INPUT"),
        "expected NEEDS_HUMAN_INPUT marker; got: {stderr}"
    );

    let update = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "update",
            "--clear-questions",
            "--clear-unknowns",
        ],
    );
    assert!(
        update.status.success(),
        "plan update failed: {}",
        String::from_utf8_lossy(&update.stderr)
    );

    let ok = run_decapod(
        &dir,
        &["govern", "plan", "check-execute", "--todo-id", &todo_id],
    );
    assert!(
        ok.status.success(),
        "check-execute should pass after questions are cleared: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

#[test]
fn missing_todo_reports_coordination_recovery_without_reasking_intent() {
    let (_tmp, dir, todo_id) = setup_repo();
    let init = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Preserve request context",
            "--intent",
            "Implement the already understood request",
            "--todo-id",
            &todo_id,
            "--proof-hook",
            "focused tests pass",
        ],
    );
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );
    let before = run_decapod(&dir, &["govern", "plan", "status"]);
    let missing = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "check-execute",
            "--todo-id",
            "absent-task",
        ],
    );
    assert_eq!(missing.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(stderr.contains("TODO_PROJECTION_MISSING"), "{stderr}");
    assert!(!stderr.contains("NEEDS_HUMAN_INPUT"), "{stderr}");
    let payload: serde_json::Value =
        serde_json::from_str(stderr.split_once("payload=").unwrap().1.trim()).unwrap();
    assert_eq!(payload["kind"], "coordination_projection_missing");
    assert_eq!(payload["todo_ids"], serde_json::json!(["absent-task"]));
    assert_eq!(payload["execution_ready"], false);
    assert_eq!(
        before.stdout,
        run_decapod(&dir, &["govern", "plan", "status"]).stdout
    );

    // Select the existing coordination record; no new user answer or plan
    // approval is needed, and the ordinary execution check still runs.
    let recovered = run_decapod(
        &dir,
        &["govern", "plan", "check-execute", "--todo-id", &todo_id],
    );
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert!(String::from_utf8_lossy(&recovered.stdout).contains("EXECUTION_READY"));
}
