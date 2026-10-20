use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(root: &Path, args: &[&str]) -> Output {
    run_authenticated(root, args, "")
}

fn run_authenticated(root: &Path, args: &[&str], password: &str) -> Output {
    Command::new(super::resolve_decapod_bin())
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "workunit-recovery-test")
        .env("DECAPOD_SESSION_PASSWORD", password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .args(args)
        .output()
        .expect("run CLI")
}

fn success(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn workunit_projection_recovery_is_idempotent_through_real_cli() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let init = Command::new("git")
        .current_dir(root)
        .args(["init", "-b", "master"])
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(run(root, &["init", "--force"]).status.success());
    let acquired = run(root, &["session", "acquire"]);
    assert!(acquired.status.success());
    let password = String::from_utf8_lossy(&acquired.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
        .expect("fixture session password");
    let run = |root: &Path, args: &[&str]| run_authenticated(root, args, &password);
    let task = "test_recovery_cli";
    success(run(
        root,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            task,
            "--intent-ref",
            "intent:recovery-cli",
        ],
    ));
    success(run(
        root,
        &[
            "govern",
            "workunit",
            "attach-state",
            "--task-id",
            task,
            "--ref",
            "evidence:observed",
        ],
    ));
    let original = success(run(root, &["govern", "workunit", "get", "--task-id", task]));
    let journal_before = success(run(
        root,
        &["govern", "workunit", "journal-status", "--task-id", task],
    ));
    let projection = root
        .join(".decapod/data/workunits")
        .join(format!("{task}.json"));
    fs::remove_file(&projection).unwrap();
    assert!(
        !run(root, &["govern", "workunit", "get", "--task-id", task])
            .status
            .success()
    );
    success(run(
        root,
        &["govern", "workunit", "recover", "--task-id", task],
    ));
    let restored = fs::read(&projection).unwrap();
    assert_eq!(
        success(run(root, &["govern", "workunit", "get", "--task-id", task])),
        original
    );
    success(run(
        root,
        &["govern", "workunit", "recover", "--task-id", task],
    ));
    assert_eq!(fs::read(&projection).unwrap(), restored);
    assert_eq!(
        success(run(
            root,
            &["govern", "workunit", "journal-status", "--task-id", task]
        )),
        journal_before
    );
    // Projection recovery cannot upgrade this draft to verified work.
    assert_eq!(original["status"], "DRAFT");
}

#[test]
fn checkpoint_and_resume_contracts_require_explicit_request_identity() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["govern", "workunit", "checkpoint", "--task-id", "test_a"],
        vec!["govern", "workunit", "resume", "--task-id", "test_a"],
        vec!["govern", "trajectory", "checkpoint", "--run-id", "run_a"],
        vec!["govern", "trajectory", "verify-resume", "--run-id", "run_a"],
    ] {
        let output = run(temp.path(), &args);
        assert_eq!(output.status.code(), Some(2));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("--operation-id") || error.contains("--checkpoint-id"),
            "{error}"
        );
    }
    assert!(!temp.path().join(".decapod").exists());
}
