use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn run_decapod(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run decapod")
}

#[test]
fn init_scaffolds_github_action_workflow() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "--proof"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let workflow_path = tmp.path().join(".github/workflows/decapod-validate.yml");
    assert!(
        workflow_path.exists(),
        "expected .github/workflows/decapod-validate.yml to exist"
    );

    let content = fs::read_to_string(workflow_path).expect("read workflow file");
    assert!(content.contains("name: Decapod Validate"));
    assert!(content.contains("decapod validate"));
    assert!(content.contains("decapod init --proof"));
    assert!(content.contains("DECAPOD_VALIDATE_SKIP_GIT_GATES: 1"));
    assert!(content.contains("on:"));
    assert!(content.contains("push:"));
    assert!(content.contains("pull_request:"));
}

#[test]
fn init_force_updates_existing_workflow() {
    let tmp = tempdir().expect("tempdir");
    let workflow_dir = tmp.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).expect("create workflow dir");
    let workflow_path = workflow_dir.join("decapod-validate.yml");
    fs::write(&workflow_path, "old content").expect("write old content");

    // Without --force, it should not overwrite (but decapod init might not fail if it's just one file)
    // Actually decapod init --proof might skip it or fail.
    // Let's check if --force overwrites it.
    let out = run_decapod(tmp.path(), &["init", "--proof", "--force"]);
    assert!(
        out.status.success(),
        "decapod init --force failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(workflow_path).expect("read workflow file");
    assert!(
        content.contains("name: Decapod Validate"),
        "workflow should be updated with --force"
    );
}

#[test]
fn init_no_ci_skips_workflow() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "--proof", "--no-ci"]);
    assert!(
        out.status.success(),
        "decapod init --no-ci failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let workflow_path = tmp.path().join(".github/workflows/decapod-validate.yml");
    assert!(
        !workflow_path.exists(),
        "expected .github/workflows/decapod-validate.yml NOT to exist with --no-ci"
    );
}
