use decapod::core::capsule_policy::CapsulePolicyBinding;
use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule, write_context_capsule,
};
use decapod::core::{trajectory, workspace, workunit};
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
                evaluator_epoch: None,
                validation_epoch: None,
            })
            .collect(),
        validation_epoch: None,
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
            path: "interfaces/PLAN_GOVERNED_EXECUTION".to_string(),
            section: "Contract".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/PLAN_GOVERNED_EXECUTION".to_string(),
            text: "promotion path is proof-gated".to_string(),
        }],
        capabilities: vec![],
        policy: CapsulePolicyBinding {
            risk_tier: "medium".to_string(),
            policy_hash: "abc123".to_string(),
            policy_version: "jit-capsule-policy-v1".to_string(),
            policy_path: ".decapod/generated/policy/context_capsule_policy.json".to_string(),
            repo_revision: "UNBORN:master".to_string(),
        },
        capsule_hash: String::new(),
        repo_signal_fingerprint: "test_fingerprint".to_string(),
        config_input_hash: String::new(),
        spec_input_hash: String::new(),
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
        "test_000001",
        workunit::WorkUnitStatus::Claimed,
        vec![],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_000001")
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
    write_capsule(dir.path(), "test_000002");
    write_manifest(
        dir.path(),
        "test_000002",
        workunit::WorkUnitStatus::Verified,
        vec![".decapod/generated/context/test_000002.json"],
        vec!["validate_passes", "test:cargo test --all"],
        vec![
            ("validate_passes", "pass"),
            ("test:cargo test --all", "pass"),
        ],
    );

    let result = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_000002");
    assert!(result.is_ok(), "expected verified branch task to pass");
}

#[test]
fn publish_gate_fails_when_verified_task_missing_capsule_lineage() {
    let dir = tempdir().expect("tempdir");
    write_manifest(
        dir.path(),
        "test_000003",
        workunit::WorkUnitStatus::Verified,
        vec![".decapod/generated/context/test_000003.json"],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_000003")
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
    write_capsule(dir.path(), "test_000004");
    write_manifest(
        dir.path(),
        "test_000004",
        workunit::WorkUnitStatus::Verified,
        vec![],
        vec!["validate_passes"],
        vec![("validate_passes", "pass")],
    );

    let err = workspace::verify_workunit_gate_for_publish(dir.path(), "agent/codex/test_000004")
        .expect_err("expected missing capsule state_ref failure");
    let msg = err.to_string();
    assert!(
        msg.contains("WORKUNIT_CAPSULE_POLICY_LINEAGE_STATE_REF_MISSING"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn trajectory_gate_replaces_workunit_requirement_for_publication() {
    let dir = tempdir().expect("tempdir");
    trajectory::init_trajectory(
        dir.path(),
        trajectory::TrajectoryInit {
            run_id: "run_publish".to_string(),
            task_id: Some("test_000005".to_string()),
            intent_id: None,
            original_intent: "publish a governed change".to_string(),
            derived_intent: "bind publication to the trajectory cookie".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/lib.rs".to_string()],
            destination: Some("single PR".to_string()),
            current_phase: Some("proof".to_string()),
            next_transitions: vec!["publish".to_string()],
            blockers: vec![],
        },
    )
    .expect("initialize trajectory");
    trajectory::record_trajectory(
        dir.path(),
        "run_publish",
        trajectory::TrajectoryUpdate {
            checks: vec![trajectory::TrajectoryCheck {
                name: "validate".to_string(),
                status: trajectory::TrajectoryCheckStatus::Passed,
            }],
            ..Default::default()
        },
    )
    .expect("record trajectory proof");

    workspace::verify_trajectory_gate_for_publish(dir.path(), "agent/codex/test_000005")
        .expect("trajectory-bound branch should pass without a workunit manifest");
}
