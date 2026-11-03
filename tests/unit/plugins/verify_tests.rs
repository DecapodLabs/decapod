// Moved from src/decapod/plugins/verify.rs
use super::{
    resolve_artifact_path_for_todo, validation_epoch_changed_reason, validation_proof_reason,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn stable_failed_validation_remains_blocked_with_explicit_reason() {
    assert_eq!(
        validation_proof_reason(false, true, "bugs_01test"),
        "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn failed_validation_with_changed_output_reports_validation_failure() {
    assert_eq!(
        validation_proof_reason(false, false, "bugs_01test"),
        "decapod validate did not pass. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn passing_validation_with_changed_output_reports_hash_drift() {
    assert_eq!(
        validation_proof_reason(true, false, "bugs_01test"),
        "validate output hash changed. Next: review the drift, then recapture with `decapod qa verify regen bugs_01test` if the change is intentional"
    );
}

#[test]
fn stale_epoch_reports_supported_aggregate_deadlock_recovery() {
    let reason = validation_epoch_changed_reason("epoch-old", "epoch-new", "bugs_01test");

    assert!(reason.contains("decapod qa verify recover"));
    assert!(reason.contains("--artifact bugs_01test=<current-file>"));
    assert!(reason.contains("without reopening"));
    assert!(reason.contains("actual passing validation"));
    assert!(!reason.contains("qa verify prune"));
    assert!(!reason.contains("todo claim"));
    assert!(!reason.contains("todo done"));
}

#[test]
fn artifact_path_resolves_inside_task_workspace_when_missing_from_parent() {
    let tmp = tempdir().expect("tmpdir");
    let host = tmp.path();
    let workspace = host
        .join(".decapod")
        .join("workspaces")
        .join("agent-unknown-bugs-01kzartxxx");
    fs::create_dir_all(workspace.join("tests")).expect("ws tests");
    fs::write(workspace.join("tests/storage_fixtures.rs"), "fn t() {}\n").expect("fixture");

    let resolved =
        resolve_artifact_path_for_todo(host, Some("bugs_01kzartxxx"), "tests/storage_fixtures.rs");
    assert_eq!(
        resolved,
        workspace.join("tests/storage_fixtures.rs"),
        "parent checkout must find the workspace-only artifact"
    );
}

#[test]
fn failed_child_diagnostics_are_bounded_fixed_labels_not_raw_output() {
    let diagnostic = super::safe_validation_failure_diagnostic(
        b"NEEDS_PLAN_APPROVAL: private task text secret=do-not-forward",
        b"READINESS_BLOCKED: https://secret.invalid/token?credential=private",
    );
    assert!(diagnostic.contains("NEEDS_PLAN_APPROVAL"));
    assert!(diagnostic.contains("READINESS_BLOCKED"));
    assert!(diagnostic.contains("decapod validate --format json"));
    assert!(!diagnostic.contains("private"));
    assert!(!diagnostic.contains("credential"));
    assert!(!diagnostic.contains("secret.invalid"));
    assert!(diagnostic.len() < 1024);

    let unknown = super::safe_validation_failure_diagnostic(b"unknown secret", b"raw error");
    assert!(unknown.contains("no recognized safe blocker label"));
    assert!(!unknown.contains("unknown secret"));
    let mut oversized = vec![b'x'; 1024 * 1024];
    oversized.extend_from_slice(b"NEEDS_PLAN_APPROVAL");
    assert!(
        !super::safe_validation_failure_diagnostic(&oversized, b"").contains("NEEDS_PLAN_APPROVAL")
    );
}
