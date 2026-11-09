//! Inspect intent without a session or agent-owned worktree.
use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn intent_reconcile_rpc_reports_gaps_without_mutating_specs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let git = Command::new("git")
        .current_dir(root)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(
        git.status.success(),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );

    let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args([
            "init",
            "--force",
            "--product-summary",
            "Reconciliation Fixture preserves explicit user goals in project code.",
            "--no-container-workspaces",
            "--isolation-mode",
            "worktree",
        ])
        .output()
        .expect("decapod init");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let path = root.join(".decapod/managed/specs/INTENT.md");
    let before = fs::read(&path).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args(["rpc", "--op", "intent.reconcile", "--params", "{}"])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .expect("intent reconcile");
    assert!(
        result.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["success"], true);
    assert_eq!(
        response["result"]["report"]["semantic_alignment"],
        "not_proven"
    );
    assert!(
        response["result"]["report"]["specs"]
            .as_array()
            .unwrap()
            .len()
            >= 7
    );
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn intent_reconcile_rpc_immutable_base_reports_code_change_as_unproven() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let git = |args: &[&str]| -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-b", "master"]);
    git(&["config", "user.name", "Decapod Test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args([
            "init",
            "--force",
            "--product-summary",
            "Impact Fixture has immutable human intent and tracked source.",
            "--no-container-workspaces",
            "--isolation-mode",
            "worktree",
        ])
        .output()
        .expect("decapod init");
    assert!(init.status.success(), "{:?}", String::from_utf8_lossy(&init.stderr));
    git(&["add", "-A"]);
    git(&["-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
        "commit", "-m", "baseline"]);
    let base = git(&["rev-parse", "HEAD"]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/intent_impact.rs"), "pub fn changed() {}\n").unwrap();
    git(&["add", "src/intent_impact.rs"]);
    git(&["-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
        "commit", "-m", "implementation without spec authoring"]);
    let params = serde_json::json!({"base_commit": base}).to_string();
    let result = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args(["rpc", "--op", "intent.reconcile", "--params", &params])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .expect("intent reconcile with immutable base");
    assert!(
        result.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["success"], true);
    assert_eq!(response["result"]["report"]["schema_version"], "1.1.0");
    let impact = &response["result"]["report"]["change_impact"];
    assert_eq!(impact["complete"], true);
    assert_eq!(impact["review_needed"], true);
    assert!(impact["unmapped_implementation_paths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "src/intent_impact.rs"));
    assert_eq!(response["result"]["report"]["semantic_alignment"], "not_proven");

    let rejected = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args(["rpc", "--op", "intent.reconcile", "--params", r#"{"base_commit":"HEAD"}"#])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .expect("reject moving base");
    assert!(!rejected.status.success(), "a symbolic base must never be accepted");
}
