use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
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
fn ordered_phases_require_artifacts_and_reject_skipped_transitions() {
    let (_tmp, dir, todo_id) = setup_repo();

    let init_plan = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Phase gate fixture",
            "--intent",
            "Demonstrate deterministic phase entry",
            "--todo-id",
            &todo_id,
        ],
    );
    assert!(init_plan.status.success());
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );

    for phase in ["context", "implementation"] {
        let add = run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "phase",
                "add",
                "--id",
                phase,
                "--require-artifact",
                "evidence/context.json",
                "--remediation",
                "Create the required evidence artifact before continuing.",
            ],
        );
        assert!(
            add.status.success(),
            "phase add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        );
    }

    let missing_artifact = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "context"],
    );
    assert!(!missing_artifact.status.success());
    assert!(String::from_utf8_lossy(&missing_artifact.stderr).contains("PHASE_GATE_FAILED"));

    std::fs::create_dir_all(dir.join("evidence")).expect("evidence dir");
    std::fs::write(dir.join("evidence/context.json"), "{}\n").expect("evidence artifact");

    let first = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "context"],
    );
    assert!(
        first.status.success(),
        "first phase should enter: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let repeated_enter = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "context"],
    );
    assert!(
        repeated_enter.status.success(),
        "entering the active phase must be idempotent: {}",
        String::from_utf8_lossy(&repeated_enter.stderr)
    );

    let complete_first = run_decapod(
        &dir,
        &["govern", "plan", "phase", "complete", "--id", "context"],
    );
    assert!(
        complete_first.status.success(),
        "active phase should complete: {}",
        String::from_utf8_lossy(&complete_first.stderr)
    );

    let repeated_complete = run_decapod(
        &dir,
        &["govern", "plan", "phase", "complete", "--id", "context"],
    );
    assert!(
        repeated_complete.status.success(),
        "completing an already terminal phase must be idempotent: {}",
        String::from_utf8_lossy(&repeated_complete.stderr)
    );

    let second = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "implementation"],
    );
    assert!(
        second.status.success(),
        "next ordered phase should enter: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let backwards = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "context"],
    );
    assert!(!backwards.status.success());
    assert!(String::from_utf8_lossy(&backwards.stderr).contains("INVALID_PHASE_TRANSITION"));
}

#[test]
fn phase_definition_rejects_conflicts_and_unknown_todos() {
    let (_tmp, dir, todo_id) = setup_repo();
    let init_plan = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Definition validation fixture",
            "--intent",
            "Reject invalid phase contracts before execution",
            "--todo-id",
            &todo_id,
        ],
    );
    assert!(init_plan.status.success());

    let first = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "phase",
            "add",
            "--id",
            "proof",
            "--require-artifact",
            "evidence/proof.json",
        ],
    );
    assert!(first.status.success());

    let repeated = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "phase",
            "add",
            "--id",
            "proof",
            "--require-artifact",
            "evidence/proof.json",
        ],
    );
    assert!(
        repeated.status.success(),
        "identical phase definitions must be idempotent: {}",
        String::from_utf8_lossy(&repeated.stderr)
    );

    let duplicate = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "phase",
            "add",
            "--id",
            "proof",
            "--require-artifact",
            "evidence/different.json",
        ],
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("PHASE_CONFLICT"));

    let unknown_todo = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "phase",
            "add",
            "--id",
            "unknown-todo",
            "--require-verified-todo",
            "code_missing",
        ],
    );
    assert!(!unknown_todo.status.success());
    assert!(String::from_utf8_lossy(&unknown_todo.stderr).contains("UNKNOWN_PHASE_TODO"));
}

#[test]
fn phase_entry_requires_decapod_verified_todo_evidence() {
    let (_tmp, dir, todo_id) = setup_repo();
    let init_plan = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Proof phase fixture",
            "--intent",
            "Require independently recorded proof",
            "--todo-id",
            &todo_id,
        ],
    );
    assert!(init_plan.status.success());
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );

    let add = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "phase",
            "add",
            "--id",
            "complete",
            "--require-verified-todo",
            &todo_id,
        ],
    );
    assert!(add.status.success());

    let blocked = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "complete"],
    );
    assert!(!blocked.status.success());
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(stderr.contains("PHASE_GATE_FAILED"));
    assert!(stderr.contains("TODO lacks a completed Decapod verification record"));
}

#[test]
fn todo_completion_cannot_bypass_declared_phase_lifecycle() {
    let (_tmp, dir, todo_id) = setup_repo();
    let init_plan = run_decapod(
        &dir,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Bypass fixture",
            "--intent",
            "Require an active phase before completion",
            "--todo-id",
            &todo_id,
        ],
    );
    assert!(init_plan.status.success());
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );
    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "add", "--id", "implementation",],
        )
        .status
        .success()
    );

    let done = run_decapod(&dir, &["todo", "done", "--id", &todo_id]);
    assert!(!done.status.success());
    assert!(String::from_utf8_lossy(&done.stderr).contains("PHASE_REQUIRED"));
}

#[test]
fn concurrent_phase_entry_is_serialized_and_idempotent() {
    let (_tmp, dir, todo_id) = setup_repo();
    assert!(
        run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Concurrent phase fixture",
                "--intent",
                "Serialize phase mutations",
                "--todo-id",
                &todo_id,
            ],
        )
        .status
        .success()
    );
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );
    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "add", "--id", "implementation",],
        )
        .status
        .success()
    );

    let barrier = Arc::new(Barrier::new(2));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let dir = dir.clone();
        let barrier = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            barrier.wait();
            run_decapod(
                &dir,
                &["govern", "plan", "phase", "enter", "--id", "implementation"],
            )
        }));
    }
    for join in joins {
        let output = join.join().expect("phase entry thread");
        assert!(
            output.status.success(),
            "concurrent entry should serialize or observe idempotent state: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let status = run_decapod(&dir, &["govern", "plan", "status"]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).expect("status json");
    assert_eq!(status["plan"]["active_phase"], "implementation");
}

#[test]
fn ordered_phase_lifecycle_reaches_done_only_after_final_completion() {
    let (_tmp, dir, todo_id) = setup_repo();
    assert!(
        run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Complete lifecycle fixture",
                "--intent",
                "Exercise entry and exit phase gates",
                "--todo-id",
                &todo_id,
            ],
        )
        .status
        .success()
    );
    assert!(
        run_decapod(&dir, &["govern", "plan", "approve"])
            .status
            .success()
    );
    assert!(
        run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "phase",
                "add",
                "--id",
                "context",
                "--exit-require-artifact",
                "evidence/exit.json",
            ],
        )
        .status
        .success()
    );
    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "add", "--id", "implementation",],
        )
        .status
        .success()
    );

    let out_of_order = run_decapod(
        &dir,
        &["govern", "plan", "phase", "enter", "--id", "implementation"],
    );
    assert!(!out_of_order.status.success());
    assert!(String::from_utf8_lossy(&out_of_order.stderr).contains("INVALID_PHASE_TRANSITION"));

    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "enter", "--id", "context"],
        )
        .status
        .success()
    );
    let missing_exit = run_decapod(
        &dir,
        &["govern", "plan", "phase", "complete", "--id", "context"],
    );
    assert!(!missing_exit.status.success());
    assert!(String::from_utf8_lossy(&missing_exit.stderr).contains("PHASE_EXIT_GATE_FAILED"));

    std::fs::create_dir_all(dir.join("evidence")).expect("evidence dir");
    std::fs::write(dir.join("evidence/exit.json"), "{}\n").expect("exit evidence");
    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "complete", "--id", "context"],
        )
        .status
        .success()
    );
    assert!(
        run_decapod(
            &dir,
            &["govern", "plan", "phase", "enter", "--id", "implementation",],
        )
        .status
        .success()
    );

    let before_final = run_decapod(&dir, &["govern", "plan", "status"]);
    let before_final: serde_json::Value =
        serde_json::from_slice(&before_final.stdout).expect("status json");
    assert_ne!(before_final["plan"]["state"], "DONE");

    assert!(
        run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "phase",
                "complete",
                "--id",
                "implementation",
            ],
        )
        .status
        .success()
    );
    let done = run_decapod(&dir, &["govern", "plan", "status"]);
    let done: serde_json::Value = serde_json::from_slice(&done.stdout).expect("status json");
    assert_eq!(done["plan"]["state"], "DONE");
}
