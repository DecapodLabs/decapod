use decapod::core::capsule_policy::CapsulePolicyBinding;
use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule, write_context_capsule,
};
use decapod::core::{workspace, workunit};
use tempfile::tempdir;

fn write_manifest(
    root: &std::path::Path,
    task_id: &str,
    status: workunit::WorkUnitStatus,
    state_refs: Vec<&str>,
    proof_plan: Vec<&str>,
    proof_results: Vec<(&str, &str)>,
) {
    let manifest = workunit::WorkUnitManifest {
        task_id: task_id.to_string(),
        intent_ref: "intent://demo".to_string(),
        spec_refs: vec![],
        state_refs: state_refs.into_iter().map(|s| s.to_string()).collect(),
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

fn write_capsule(root: &std::path::Path, task_id: &str) {
    let capsule = DeterministicContextCapsule {
        schema_version: "1.1.0".to_string(),
        topic: "publish".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some(task_id.to_string()),
        workunit_id: None,
        sources: vec![ContextCapsuleSource {
            path: "interfaces/PLAN_GOVERNED_EXECUTION.md".to_string(),
            section: "Contract".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/PLAN_GOVERNED_EXECUTION.md".to_string(),
            text: "promotion path is proof-gated".to_string(),
        }],
        policy: CapsulePolicyBinding {
            risk_tier: "medium".to_string(),
            policy_hash: "abc123".to_string(),
            policy_version: "jit-capsule-policy-v1".to_string(),
            policy_path: ".decapod/generated/policy/context_capsule_policy.json".to_string(),
            repo_revision: "UNBORN:master".to_string(),
        },
        capsule_hash: String::new(),
    };
    write_context_capsule(root, &capsule).expect("write capsule");
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
        "test_01",
        workunit::WorkUnitStatus::Claimed,
        vec![],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_01")
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
    write_capsule(dir.path(), "test_02");
    write_manifest(
        dir.path(),
        "test_02",
        workunit::WorkUnitStatus::Verified,
        vec![".decapod/generated/context/test_02.json"],
        vec!["validate_passes", "test:cargo test --all"],
        vec![
            ("validate_passes", "pass"),
            ("test:cargo test --all", "pass"),
        ],
    );

    let result = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_02");
    assert!(result.is_ok(), "expected verified branch task to pass");
}

#[test]
fn publish_gate_fails_when_verified_task_missing_capsule_lineage() {
    let dir = tempdir().expect("tempdir");
    write_manifest(
        dir.path(),
        "test_03",
        workunit::WorkUnitStatus::Verified,
        vec![".decapod/generated/context/test_03.json"],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_03")
        .expect_err("expected missing capsule lineage failure");
    let msg = err.to_string();
    assert!(
        msg.contains("WORKUNIT_CAPSULE_POLICY_LINEAGE_MISSING"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn publish_gate_fails_when_verified_task_capsule_state_ref_missing() {
    let dir = tempdir().expect("tempdir");
    write_capsule(dir.path(), "test_04");
    write_manifest(
        dir.path(),
        "test_04",
        workunit::WorkUnitStatus::Verified,
        vec![],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_04")
        .expect_err("expected missing capsule state_ref failure");
    let msg = err.to_string();
    assert!(
        msg.contains("WORKUNIT_CAPSULE_POLICY_LINEAGE_STATE_REF_MISSING"),
        "unexpected error message: {msg}"
    );
}
