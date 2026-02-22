use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_cmd(repo_root: &Path, args: &[&str]) -> Output {
    let exe = env!("CARGO_BIN_EXE_decapod");
    Command::new(exe)
        .current_dir(repo_root)
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .unwrap_or_else(|e| panic!("failed to run decapod {:?}: {}", args, e))
}

#[allow(dead_code)]
fn init_git_repo(path: &Path) {
    Command::new("git")
        .current_dir(path)
        .args(["init"])
        .output()
        .expect("failed to init git repo");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("failed to config git email");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.name", "test"])
        .output()
        .expect("failed to config git name");
    Command::new("git")
        .current_dir(path)
        .args(["add", "."])
        .output()
        .expect("failed to git add");
    Command::new("git")
        .current_dir(path)
        .args(["commit", "-m", "initial"])
        .output()
        .expect("failed to git commit");
}

fn extract_json(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout
        .find('{')
        .unwrap_or_else(|| panic!("expected JSON output, got: {}", stdout));
    serde_json::from_str(&stdout[json_start..]).unwrap_or_else(|e| {
        panic!(
            "failed to parse JSON output: {}\nstdout:\n{}\nstderr:\n{}",
            e,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
#[ignore = "test needs investigation - fails on master due to verification setup issues"]
fn verify_mvp_pass_fail_unknown_flow() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();

    init_git_repo(repo);

    // A) init + create validated TODO with baseline artifacts
    let init = run_cmd(repo, &["init", "--dir", "."]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = run_cmd(repo, &["session", "acquire"]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );

    let add = run_cmd(
        repo,
        &[
            "todo",
            "add",
            "Verify MVP target",
            "--dir",
            ".",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_json = extract_json(&add);
    let todo_id = add_json["id"].as_str().unwrap().to_string();

    let done_validated = run_cmd(repo, &["todo", "done", "--id", &todo_id, "--validated"]);
    assert!(
        done_validated.status.success(),
        "todo done --validated failed: {}",
        String::from_utf8_lossy(&done_validated.stderr)
    );

    let verify_pass = run_cmd(repo, &["qa", "verify", "--json"]);
    assert!(
        verify_pass.status.success(),
        "verify pass run failed: {}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );
    let pass_json = extract_json(&verify_pass);
    assert_eq!(pass_json["summary"]["passed"], 1);
    assert_eq!(pass_json["summary"]["failed"], 0);
    assert_eq!(pass_json["summary"]["unknown"], 0);

    // B) Tamper AGENTS.md and verify failure includes expected vs actual hash details.
    fs::OpenOptions::new()
        .append(true)
        .open(repo.join("AGENTS.md"))
        .unwrap()
        .write_all(b"\n# tamper\n")
        .unwrap();

    let verify_fail = run_cmd(repo, &["qa", "verify", "--json"]);
    assert!(
        !verify_fail.status.success(),
        "verify should fail after AGENTS.md tamper"
    );
    let fail_json = extract_json(&verify_fail);
    let failed = fail_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["todo_id"] == todo_id)
        .expect("missing failed todo result");
    assert_eq!(failed["status"], "fail");
    let artifact_fail = failed["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["path"] == "AGENTS.md")
        .expect("missing AGENTS.md artifact failure");
    assert!(artifact_fail["expected_hash"].as_str().is_some());
    assert!(artifact_fail["actual_hash"].as_str().is_some());
    assert_ne!(artifact_fail["expected_hash"], artifact_fail["actual_hash"]);

    // C) validated TODO lacking verification artifacts should be UNKNOWN with remediation.
    let add_unknown = run_cmd(
        repo,
        &[
            "todo",
            "add",
            "Missing verify artifacts",
            "--dir",
            ".",
            "--format",
            "json",
        ],
    );
    let add_unknown_json = extract_json(&add_unknown);
    let unknown_id = add_unknown_json["id"].as_str().unwrap().to_string();

    let done_plain = run_cmd(repo, &["todo", "done", "--id", &unknown_id, "--validated"]);
    assert!(
        done_plain.status.success(),
        "todo done failed: {}",
        String::from_utf8_lossy(&done_plain.stderr)
    );

    let db = Connection::open(repo.join(".decapod/data/todo.db")).unwrap();
    let (status, notes, artifacts_json): (String, String, String) = db
        .query_row(
            "SELECT last_verified_status, last_verified_notes, verification_artifacts
             FROM task_verification WHERE todo_id = ?1",
            rusqlite::params![unknown_id.clone()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert!(
        status == "pass" || status == "fail",
        "expected baseline status pass|fail, got {}",
        status
    );
    assert!(
        notes.contains("baseline captured"),
        "expected baseline capture note, got: {}",
        notes
    );
    let artifacts: Value = serde_json::from_str(&artifacts_json).unwrap();
    assert_eq!(
        artifacts["proof_plan_results"][0]["proof_gate"],
        "validate_passes"
    );
    let proof_status = artifacts["proof_plan_results"][0]["status"]
        .as_str()
        .unwrap_or_default();
    assert!(
        proof_status == "pass" || proof_status == "fail",
        "expected proof status pass|fail, got {}",
        proof_status
    );

    db.execute(
        "UPDATE task_verification SET verification_artifacts = NULL WHERE todo_id = ?1",
        rusqlite::params![unknown_id.clone()],
    )
    .unwrap();

    let verify_unknown = run_cmd(repo, &["qa", "verify", "--json", "todo", &unknown_id]);
    assert!(
        verify_unknown.status.success(),
        "verify todo unknown run should not hard-fail: {}",
        String::from_utf8_lossy(&verify_unknown.stderr)
    );
    let unknown_json = extract_json(&verify_unknown);
    let unknown_result = unknown_json["results"].as_array().unwrap().first().unwrap();
    assert_eq!(unknown_result["todo_id"], unknown_id);
    assert_eq!(unknown_result["status"], "unknown");
    let notes = unknown_result["notes"].as_array().unwrap();
    let has_remediation = notes
        .iter()
        .filter_map(|n| n.as_str())
        .any(|n| n.contains("Remediation") || n.contains("capture"));
    assert!(has_remediation, "expected remediation guidance in notes");
}
