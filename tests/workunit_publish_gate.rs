use decapod::core::{workspace, workunit};
use tempfile::tempdir;

fn write_manifest(
    root: &std::path::Path,
    task_id: &str,
    status: workunit::WorkUnitStatus,
    proof_plan: Vec<&str>,
    proof_results: Vec<(&str, &str)>,
) {
    let manifest = workunit::WorkUnitManifest {
        task_id: task_id.to_string(),
        intent_ref: "intent://demo".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: proof_plan.into_iter().map(|s| s.to_string()).collect(),
        proof_results: proof_results
            .into_iter()
            .map(|(gate, status)| workunit::WorkUnitProofResult {
                gate: gate.to_string(),
                status: status.to_string(),
                artifact_ref: None,
            })
            .collect(),
        status,
    };

    workunit::write_workunit(root, &manifest).expect("write workunit manifest");
}

#[test]
fn publish_gate_skips_when_branch_has_no_task_ids() {
    let dir = tempdir().expect("tempdir");
    let result = workspace::verify_workunit_gate_for_publish(dir.path(), "feature/no-task-id");
    assert!(result.is_ok(), "expected no-op pass for non-task branch");
}

#[test]
fn publish_gate_fails_when_branch_task_manifest_missing() {
    let dir = tempdir().expect("tempdir");
    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/unknown/r_01ABCXYZ")
        .expect_err("expected missing workunit manifest failure");
    let msg = err.to_string();
    assert!(
        msg.contains("missing required workunit manifest"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn publish_gate_fails_when_branch_task_not_verified() {
    let dir = tempdir().expect("tempdir");
    write_manifest(
        dir.path(),
        "R_01ABCD1",
        workunit::WorkUnitStatus::Claimed,
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/r_01ABCD1")
        .expect_err("expected status gate failure");
    let msg = err.to_string();
    assert!(
        msg.contains("is not VERIFIED"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn publish_gate_passes_when_branch_task_verified() {
    let dir = tempdir().expect("tempdir");
    write_manifest(
        dir.path(),
        "R_01ABCD2",
        workunit::WorkUnitStatus::Verified,
        vec!["validate_passes", "test:cargo test --all"],
        vec![
            ("validate_passes", "pass"),
            ("test:cargo test --all", "pass"),
        ],
    );

    let result = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/r_01ABCD2");
    assert!(result.is_ok(), "expected verified branch task to pass");
}
