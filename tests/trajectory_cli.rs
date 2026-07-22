use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], password: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
    command
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_AGENT_ID", "trajectory-test-agent")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1");
    if let Some(password) = password {
        command.env("DECAPOD_SESSION_PASSWORD", password);
    }
    command.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let temp = TempDir::new().expect("create temp repository");
    let root = temp.path().to_path_buf();
    let git = Command::new("git")
        .current_dir(&root)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git.status.success());

    let init = run_decapod(&root, &["init", "--force"], None);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let session = run_decapod(&root, &["session", "acquire"], None);
    assert!(session.status.success());
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_string))
        .expect("session password");
    (temp, root, password)
}

fn json(output: &Output, operation: &str) -> Value {
    assert!(
        output.status.success(),
        "{operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn trajectory_cli_records_scope_actions_checks_and_verdicts() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_1",
            "--task-id",
            "task_cli_1",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "implement bounded change",
            "--boundary",
            "src/**",
            "--scope",
            "src/lib.rs",
        ],
        env_password,
    );
    let init_json = json(&init, "trajectory init");
    assert_eq!(init_json["marker"], "TRAJECTORY_INITIALIZED");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_1",
            "--inspected-file",
            "src/lib.rs",
            "--modified-file",
            "src/lib.rs",
            "--command",
            "cargo test --lib",
            "--tool-call",
            "decapod validate",
            "--check",
            "cargo_test=passed",
            "--evidence",
            ".decapod/generated/artifacts/proof.json",
            "--assumption",
            "existing interface remains compatible",
            "--completion-claim",
            "implemented and verified",
        ],
        env_password,
    );
    let record_json = json(&record, "trajectory record");
    assert_eq!(record_json["proof_status"], "passed");
    assert_eq!(record_json["verdicts"]["completion_proof"], "supported");
    assert_eq!(record_json["inspected_files"][0], "src/lib.rs");
    assert_eq!(record_json["checks"][0]["status"], "passed");

    let status = run_decapod(
        &root,
        &["govern", "trajectory", "status", "--run-id", "run_cli_1"],
        env_password,
    );
    let status_json = json(&status, "trajectory status");
    assert_eq!(status_json["proof_status"], "passed");
    assert!(
        root.join(".decapod/governance/trajectories/run_cli_1.json")
            .exists()
    );
}

#[test]
fn trajectory_cli_does_not_promote_completion_claim_without_checks() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_2",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "bounded implementation",
            "--boundary",
            "src/**",
            "--scope",
            "src/lib.rs",
        ],
        env_password,
    );
    json(&init, "trajectory init");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_2",
            "--completion-claim",
            "done",
        ],
        env_password,
    );
    let record_json = json(&record, "trajectory record");
    assert_eq!(record_json["completion_claim"], "done");
    assert_eq!(record_json["proof_status"], "no_checks_run");
    assert_eq!(record_json["verdicts"]["completion_proof"], "unsupported");
}

#[test]
fn trajectory_cli_rejects_unknown_check_status() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_3",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "bounded implementation",
        ],
        env_password,
    );
    json(&init, "trajectory init");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_3",
            "--check",
            "cargo_test=maybe",
        ],
        env_password,
    );
    assert!(!record.status.success());
    assert!(String::from_utf8_lossy(&record.stderr).contains("invalid trajectory check status"));
}

#[test]
fn generated_agent_guidance_mentions_trajectory_proof() {
    let (_temp, root, password) = setup_repo();
    let _ = password;
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("read AGENTS.md");
    assert!(agents.contains("govern trajectory init"));
    assert!(agents.contains("no_checks_run"));
}
