//! Governance readers must work without initializing a local datastore/session.
use std::process::Command;
use tempfile::TempDir;

fn empty_project() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    root
}

#[test]
fn governance_status_is_sessionless_and_has_no_migration_side_effects() {
    let root = empty_project();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root.path())
        .args(["govern", "artifacts", "status"])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.is_null());
    assert_eq!(
        std::fs::read_dir(root.path().join(".decapod"))
            .unwrap()
            .count(),
        0,
        "read-only status must not create governance, data, or session files"
    );
}

#[test]
fn governance_checkpoint_verifier_fails_without_creating_runtime_state() {
    let root = empty_project();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root.path())
        .args([
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            "missing-base",
        ])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_dir(root.path().join(".decapod"))
            .unwrap()
            .count(),
        0,
        "failed read-only verification must not initialize or migrate runtime state"
    );
}
