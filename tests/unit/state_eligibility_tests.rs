//! Regression tests for state.resolve publication eligibility bugs
//!
//! These tests verify that state.resolve correctly reports publication eligibility
// ONLY when all required evidence is present, current, valid, and properly bound.

use crate::core::workunit::WorkUnitProofResult;
use crate::core::workunit::{WorkUnitManifest, WorkUnitStatus};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper to create a minimal test workspace with Decapod structure
fn setup_test_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create .decapod structure
    fs::create_dir_all(root.join(".decapod").join("data")).unwrap();
    fs::create_dir_all(root.join(".decapod").join("artifacts")).unwrap();

    // Write minimal config.toml
    fs::write(
        root.join(".decapod").join("config.toml"),
        r#"
schema_version = "1.0.0"

[init]
specs = true
ci = true
diagram_style = "ascii"
entrypoints = []

[repo]
base_branch = "master"
"#,
    )
    .unwrap();

    // Initialize git repo
    Command::new("git")
        .current_dir(root)
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(root)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(root)
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(root)
        .args(["commit", "--allow-empty", "-m", "initial"])
        .output()
        .unwrap();

    dir
}

/// Test 1: Missing proof result should report Not Eligible
#[test]
fn test_missing_proof_result_not_eligible() {
    let workspace = setup_test_workspace();
    let root = workspace.path();

    // Create a workunit with proof plan but NO proof results
    let manifest = WorkUnitManifest {
        task_id: "test-task-1".to_string(),
        intent_ref: "test-intent".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["test_gate".to_string()],
        proof_results: vec![], // Missing!
        validation_epoch: None,
        status: WorkUnitStatus::Draft,
    };

    crate::core::todo::initialize_todo_db(&root.join(".decapod/data"))
        .expect("initialize runtime for workunit import");
    crate::core::workunit::write_workunit(root, &manifest).expect("write workunit");

    // Create identity pointing to this workunit
    let _identity = crate::core::state::StateIdentity {
        kind: crate::core::state::StateIdentityKind::Workunit,
        id: manifest.task_id.clone(),
        intent_ref: Some("test-intent".to_string()),
        task_id: Some(manifest.task_id.clone()),
    };

    // Resolve publication
    let params = crate::core::state::StateResolveParams::default();
    let resolution = crate::core::state::resolve_state(root, &params).unwrap();
    let publication = resolution.publication;

    // Should NOT be eligible - missing proof result
    assert!(
        !publication.eligible,
        "Should not be eligible with missing proof result"
    );
    assert!(
        publication
            .blocking_factors
            .iter()
            .any(|f| f.contains("proof") || f.contains("required")),
        "Should mention missing proof in blocking factors: {:?}",
        publication.blocking_factors
    );
}

/// Test 2: Failed proof result should report Not Eligible
#[test]
fn test_failed_proof_result_not_eligible() {
    let workspace = setup_test_workspace();
    let root = workspace.path();

    let manifest = WorkUnitManifest {
        task_id: "test-task-2".to_string(),
        intent_ref: "test-intent".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["test_gate".to_string()],
        proof_results: vec![WorkUnitProofResult {
            gate: "test_gate".to_string(),
            status: "fail".to_string(),
            artifact_ref: None,
            evaluator_epoch: None,
            validation_epoch: None,
        }],
        validation_epoch: None,
        status: WorkUnitStatus::Draft,
    };

    crate::core::todo::initialize_todo_db(&root.join(".decapod/data"))
        .expect("initialize runtime for workunit import");
    crate::core::workunit::write_workunit(root, &manifest).expect("write workunit");

    let _identity = crate::core::state::StateIdentity {
        kind: crate::core::state::StateIdentityKind::Workunit,
        id: manifest.task_id.clone(),
        intent_ref: Some("test-intent".to_string()),
        task_id: Some(manifest.task_id.clone()),
    };

    let params = crate::core::state::StateResolveParams::default();
    let resolution = crate::core::state::resolve_state(root, &params).unwrap();
    let publication = resolution.publication;

    // Should NOT be eligible - failed proof
    assert!(
        !publication.eligible,
        "Should not be eligible with failed proof"
    );
    assert!(
        publication
            .blocking_factors
            .iter()
            .any(|f| f.contains("fail") || f.contains("Required")),
        "Should mention failed proof in blocking factors: {:?}",
        publication.blocking_factors
    );
}

/// Test 3: State commit missing should report Not Eligible
#[test]
fn test_missing_state_commit_not_eligible() {
    let workspace = setup_test_workspace();
    let root = workspace.path();

    let manifest = WorkUnitManifest {
        task_id: "test-task-3".to_string(),
        intent_ref: "test-intent".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["test_gate".to_string()],
        proof_results: vec![WorkUnitProofResult {
            gate: "test_gate".to_string(),
            status: "pass".to_string(),
            artifact_ref: None,
            evaluator_epoch: None,
            validation_epoch: None,
        }],
        validation_epoch: None,
        status: WorkUnitStatus::Draft,
    };

    crate::core::todo::initialize_todo_db(&root.join(".decapod/data"))
        .expect("initialize runtime for workunit import");
    crate::core::workunit::write_workunit(root, &manifest).expect("write workunit");

    let _identity = crate::core::state::StateIdentity {
        kind: crate::core::state::StateIdentityKind::Workunit,
        id: manifest.task_id.clone(),
        intent_ref: Some("test-intent".to_string()),
        task_id: Some(manifest.task_id.clone()),
    };

    // No state commit file created
    let params = crate::core::state::StateResolveParams::default();
    let resolution = crate::core::state::resolve_state(root, &params).unwrap();
    let publication = resolution.publication;

    // Should NOT be eligible - missing state commit
    assert!(
        !publication.eligible,
        "Should not be eligible without state commit"
    );
    assert!(
        publication
            .blocking_factors
            .iter()
            .any(|f| f.contains("state commit") || f.contains("State commit")),
        "Should mention missing state commit in blocking factors: {:?}",
        publication.blocking_factors
    );
}

/// Test 4: Valid complete setup SHOULD be eligible
#[test]
fn test_valid_setup_is_eligible() {
    let workspace = setup_test_workspace();
    let root = workspace.path();

    let manifest = WorkUnitManifest {
        task_id: "test-task-4".to_string(),
        intent_ref: "test-intent".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["test_gate".to_string()],
        proof_results: vec![WorkUnitProofResult {
            gate: "test_gate".to_string(),
            status: "pass".to_string(),
            artifact_ref: None,
            evaluator_epoch: None,
            validation_epoch: None,
        }],
        validation_epoch: None,
        status: WorkUnitStatus::Draft,
    };

    crate::core::todo::initialize_todo_db(&root.join(".decapod/data"))
        .expect("initialize runtime for workunit import");
    crate::core::workunit::write_workunit(root, &manifest).expect("write workunit");

    // Create state commit file
    fs::write(
        root.join(".decapod")
            .join("artifacts")
            .join("state_commit.json"),
        "{}",
    )
    .unwrap();

    let _identity = crate::core::state::StateIdentity {
        kind: crate::core::state::StateIdentityKind::Workunit,
        id: manifest.task_id.clone(),
        intent_ref: Some("test-intent".to_string()),
        task_id: Some(manifest.task_id.clone()),
    };

    let params = crate::core::state::StateResolveParams::default();
    let resolution = crate::core::state::resolve_state(root, &params).unwrap();
    let publication = resolution.publication;

    // SHOULD be eligible - all requirements met
    assert!(
        publication.eligible,
        "Should be eligible with all requirements met: {:?}",
        publication.blocking_factors
    );
    assert!(
        publication.required_proofs_current,
        "required_proofs_current should be true"
    );
    assert!(
        publication.verifier_integrity_ok,
        "verifier_integrity_ok should be true"
    );
    assert!(
        publication.state_commit_present,
        "state_commit_present should be true"
    );
    assert!(
        publication.assurance_requirements_met,
        "assurance_requirements_met should be true"
    );
}

/// Test 5: No proof plan defined should not crash
#[test]
fn test_no_proof_plan_no_crash() {
    let workspace = setup_test_workspace();
    let root = workspace.path();

    let manifest = WorkUnitManifest {
        task_id: "test-task-5".to_string(),
        intent_ref: "test-intent".to_string(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec![], // Empty
        proof_results: vec![],
        validation_epoch: None,
        status: WorkUnitStatus::Draft,
    };

    crate::core::todo::initialize_todo_db(&root.join(".decapod/data"))
        .expect("initialize runtime for workunit import");
    crate::core::workunit::write_workunit(root, &manifest).expect("write workunit");

    // Create state commit file
    fs::write(
        root.join(".decapod")
            .join("artifacts")
            .join("state_commit.json"),
        "{}",
    )
    .unwrap();

    let _identity = crate::core::state::StateIdentity {
        kind: crate::core::state::StateIdentityKind::Workunit,
        id: manifest.task_id.clone(),
        intent_ref: Some("test-intent".to_string()),
        task_id: Some(manifest.task_id.clone()),
    };

    let params = crate::core::state::StateResolveParams::default();
    let resolution = crate::core::state::resolve_state(root, &params).unwrap();
    let publication = resolution.publication;

    // With no proof plan, no required proofs, so should be eligible (if other conditions met)
    assert!(
        publication.eligible || !publication.required_proofs_current,
        "Should handle empty proof plan gracefully: eligible={}, required_proofs_current={}",
        publication.eligible,
        publication.required_proofs_current
    );
}
