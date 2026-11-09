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
    assert!(git.status.success(), "{}", String::from_utf8_lossy(&git.stderr));

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
    assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));

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
    assert_eq!(response["result"]["report"]["semantic_alignment"], "not_proven");
    assert!(response["result"]["report"]["specs"].as_array().unwrap().len() >= 7);
    assert_eq!(fs::read(path).unwrap(), before);
}
