use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_cmd(repo_root: &Path, args: &[&str]) -> Output {
    let exe = env!("CARGO_BIN_EXE_decapod");
    let output = Command::new(exe)
        .current_dir(repo_root)
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .unwrap_or_else(|e| panic!("failed to run decapod {args:?}: {e}"));
    println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
    }
    output
}

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
}

fn extract_json(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect("Failed to parse JSON")
}

#[test]
fn test_infer_orientation_simple_task() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);
    run_cmd(root, &["init", "--force"]);

    // Create a dummy project structure
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    )
    .unwrap();

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "add documentation to lib.rs",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert_eq!(json["user_goal"], "add documentation to lib.rs");
    assert!(json["decision_gates"].as_array().unwrap().is_empty());
    assert!(json["next_action"].as_str().unwrap().contains("research"));
}

#[test]
fn test_infer_orientation_ambiguous_architecture() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);
    run_cmd(root, &["init", "--force"]);

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "refactor the core rpc interface",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert!(!json["decision_gates"].as_array().unwrap().is_empty());
    assert_eq!(
        json["decision_gates"][0]["decision"],
        "Architectural alignment"
    );
    assert!(json["next_action"].as_str().unwrap().contains("STOP"));
}

#[test]
fn test_infer_orientation_with_task_id() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);

    // Setup decapod project
    run_cmd(root, &["init", "--force"]);
    run_cmd(root, &["session", "acquire"]);

    // Create a task
    let add_output = run_cmd(
        root,
        &[
            "todo",
            "add",
            "fix the broken tests",
            "--scope",
            "tests/core",
        ],
    );
    assert!(add_output.status.success());

    let list_output = run_cmd(root, &["todo", "list", "--format", "json"]);
    assert!(list_output.status.success());
    let list_json = extract_json(&list_output);
    println!("LIST JSON: {list_json}");
    let task_id = list_json["items"]
        .as_array()
        .expect("items should be an array")[0]["id"]
        .as_str()
        .expect("Task should have an ID");

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--task-id",
            task_id,
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert_eq!(json["user_goal"], "fix the broken tests");
    assert_eq!(json["task_id"].as_str().unwrap(), task_id);
    assert!(
        json["allowed_scope"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "tests/core")
    );
    assert!(
        json["proof_required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("Reproduction"))
    );
}
