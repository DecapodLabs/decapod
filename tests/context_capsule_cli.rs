use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run decapod")
}

fn setup_repo() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = run_decapod(&dir, &["init", "--force"]);
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&decapod_init.stderr)
    );

    (tmp, dir)
}

#[test]
fn context_capsule_query_is_deterministic() {
    let (_tmp, dir) = setup_repo();

    let first = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "validation liveness",
            "--scope",
            "interfaces",
            "--task-id",
            "R_42",
            "--limit",
            "5",
        ],
    );
    assert!(
        first.status.success(),
        "first query failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "validation liveness",
            "--scope",
            "interfaces",
            "--task-id",
            "R_42",
            "--limit",
            "5",
        ],
    );
    assert!(
        second.status.success(),
        "second query failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let first_out = String::from_utf8_lossy(&first.stdout).to_string();
    let second_out = String::from_utf8_lossy(&second.stdout).to_string();
    assert_eq!(
        first_out, second_out,
        "query output should be byte-identical for same inputs"
    );

    let payload: Value = serde_json::from_str(&first_out).expect("parse output json");
    assert_eq!(payload["topic"], "validation liveness");
    assert_eq!(payload["scope"], "interfaces");
    assert!(
        !payload["capsule_hash"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let sources = payload["sources"].as_array().expect("sources array");
    assert!(!sources.is_empty(), "expected at least one source");
    for source in sources {
        let path = source["path"].as_str().unwrap_or_default();
        assert!(
            path.starts_with("interfaces/"),
            "scope filter violated, got source path: {}",
            path
        );
    }
}

#[test]
fn context_capsule_query_rejects_invalid_scope() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "foo",
            "--scope",
            "methodology",
        ],
    );

    assert!(!out.status.success(), "query should fail for invalid scope");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid scope"),
        "expected invalid scope error in stderr, got: {}",
        stderr
    );
}
