// Moved from src/decapod/core/work_claim.rs
use super::*;

fn task(status: &str, assigned_to: &str) -> Task {
    Task {
        id: "feat_01claim00000001".to_string(),
        hash: "01clai".to_string(),
        title: "bounded work".to_string(),
        description: String::new(),
        tags: String::new(),
        owner: assigned_to.to_string(),
        due: None,
        r#ref: "github:686".to_string(),
        status: status.to_string(),
        created_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:01:00Z".to_string(),
        completed_at: None,
        closed_at: None,
        dir_path: "src/decapod/core/todo.rs".to_string(),
        scope: "src/**".to_string(),
        parent_task_id: None,
        priority: "medium".to_string(),
        depends_on: String::new(),
        blocks: String::new(),
        category: "feature".to_string(),
        component: String::new(),
        assigned_to: assigned_to.to_string(),
        assigned_at: None,
        owners: Vec::new(),
        comments: Vec::new(),
        one_shot: 0,
        revision: 0,
    }
}

fn bound_verification(task: &Task) -> WorkClaimVerification {
    WorkClaimVerification {
        last_verified_status: Some("pass".to_string()),
        verification_artifacts: Some(serde_json::json!({
            "task_id": task.id,
            "task_revision": task.revision,
            "proof_plan_results": [{
                "proof_gate": "validate_passes",
                "output_hash": "sha256:deadbeef"
            }],
            "file_artifacts": [{
                "path": "AGENTS.md",
                "hash": "sha256:cafebabe"
            }]
        })),
    }
}

#[test]
fn legacy_todo_mapping_preserves_scope_and_ownership() {
    let claim = from_todo(
        &task("open", "agent-a"),
        &WorkClaimVerification::default(),
        Some("run-1".to_string()),
    );

    assert_eq!(claim.claim_id, "todo:feat_01claim00000001");
    assert_eq!(claim.intent_id, "intent:todo:feat_01claim00000001");
    assert_eq!(claim.status, WorkClaimStatus::Active);
    assert_eq!(claim.agent.as_deref(), Some("agent-a"));
    assert_eq!(claim.trajectory_id.as_deref(), Some("run-1"));
    assert_eq!(claim.paths, vec!["src/decapod/core/todo.rs"]);
    assert_eq!(claim.validation_status, WorkClaimValidationStatus::Missing);
}

#[test]
fn failed_validation_blocks_done_claim_and_keeps_proof_refs() {
    let verification = WorkClaimVerification {
        last_verified_status: Some("fail".to_string()),
        verification_artifacts: Some(serde_json::json!({
            "proof_plan_results": [{
                "proof_gate": "validate_passes",
                "output_hash": "sha256:deadbeef"
            }],
            "file_artifacts": [{
                "path": "AGENTS.md",
                "hash": "sha256:cafebabe"
            }]
        })),
    };
    let claim = from_todo(&task("done", "agent-a"), &verification, None);

    assert_eq!(claim.status, WorkClaimStatus::Blocked);
    assert_eq!(claim.validation_status, WorkClaimValidationStatus::Failed);
    assert_eq!(
        claim.proof_refs,
        vec![
            "artifact:AGENTS.md:sha256:cafebabe",
            "proof:feat_01claim00000001:validate_passes:sha256:deadbeef"
        ]
    );
}

#[test]
fn passed_validation_completes_done_claim_and_archive_abandons() {
    let verification = bound_verification(&task("done", "agent-a"));
    assert_eq!(
        from_todo(&task("done", "agent-a"), &verification, None).status,
        WorkClaimStatus::Complete
    );
    assert_eq!(
        from_todo(&task("archived", "agent-a"), &verification, None).status,
        WorkClaimStatus::Abandoned
    );
}

#[test]
fn current_task_binding_accepts_passing_status_aliases() {
    let mut task = task("done", "agent-a");
    task.revision = 7;
    for status in ["pass", "passed", "verified", "PASS", "VERIFIED"] {
        let mut verification = bound_verification(&task);
        verification.last_verified_status = Some(status.to_string());
        let claim = from_todo(&task, &verification, None);

        assert_eq!(claim.status, WorkClaimStatus::Complete, "{status}");
        assert_eq!(
            claim.validation_status,
            WorkClaimValidationStatus::Passed,
            "{status}"
        );
        assert_eq!(claim.proof_refs.len(), 2, "{status}");
    }
}

#[test]
fn passing_status_without_an_artifact_object_is_stale() {
    let task = task("done", "agent-a");
    for (label, artifacts) in [
        ("missing", None),
        ("null", Some(serde_json::Value::Null)),
        ("string", Some(serde_json::json!("legacy evidence"))),
        ("array", Some(serde_json::json!([]))),
        ("empty object", Some(serde_json::json!({}))),
    ] {
        let verification = WorkClaimVerification {
            last_verified_status: Some("pass".to_string()),
            verification_artifacts: artifacts,
        };
        let claim = from_todo(&task, &verification, None);

        assert_eq!(claim.status, WorkClaimStatus::Blocked, "{label}");
        assert_eq!(
            claim.validation_status,
            WorkClaimValidationStatus::Stale,
            "{label}"
        );
        assert!(claim.proof_refs.is_empty(), "{label}");
    }
}

#[test]
fn invalid_task_binding_is_stale_and_keeps_historical_proof_refs() {
    let mut task = task("done", "agent-a");
    task.revision = 7;
    let baseline = bound_verification(&task);
    let expected_refs = from_todo(&task, &baseline, None).proof_refs;
    for (label, binding) in [
        ("legacy unbound", serde_json::json!({})),
        ("missing task id", serde_json::json!({"task_revision": 7})),
        ("missing revision", serde_json::json!({"task_id": task.id})),
        (
            "null task id",
            serde_json::json!({"task_id": null, "task_revision": 7}),
        ),
        (
            "numeric task id",
            serde_json::json!({"task_id": 7, "task_revision": 7}),
        ),
        (
            "wrong task id",
            serde_json::json!({"task_id": "feat_other_task", "task_revision": 7}),
        ),
        (
            "null revision",
            serde_json::json!({"task_id": task.id, "task_revision": null}),
        ),
        (
            "string revision",
            serde_json::json!({"task_id": task.id, "task_revision": "7"}),
        ),
        (
            "fractional revision",
            serde_json::json!({"task_id": task.id, "task_revision": 7.5}),
        ),
        (
            "out of range revision",
            serde_json::json!({"task_id": task.id, "task_revision": u64::MAX}),
        ),
        (
            "negative revision",
            serde_json::json!({"task_id": task.id, "task_revision": -1}),
        ),
        (
            "older revision",
            serde_json::json!({"task_id": task.id, "task_revision": 6}),
        ),
        (
            "future revision",
            serde_json::json!({"task_id": task.id, "task_revision": 8}),
        ),
    ] {
        let mut artifacts = baseline.verification_artifacts.clone().unwrap();
        let object = artifacts.as_object_mut().unwrap();
        object.remove("task_id");
        object.remove("task_revision");
        object.extend(binding.as_object().unwrap().clone());
        let verification = WorkClaimVerification {
            last_verified_status: Some("pass".to_string()),
            verification_artifacts: Some(artifacts),
        };
        let claim = from_todo(&task, &verification, None);

        assert_eq!(claim.status, WorkClaimStatus::Blocked, "{label}");
        assert_eq!(
            claim.validation_status,
            WorkClaimValidationStatus::Stale,
            "{label}"
        );
        assert_eq!(claim.proof_refs, expected_refs, "{label}");
    }
}

#[test]
fn matching_negative_task_revision_cannot_authorize_completion() {
    let mut task = task("done", "agent-a");
    task.revision = -1;
    let claim = from_todo(&task, &bound_verification(&task), None);

    assert_eq!(claim.status, WorkClaimStatus::Blocked);
    assert_eq!(claim.validation_status, WorkClaimValidationStatus::Stale);
    assert_eq!(claim.proof_refs.len(), 2);
}

#[test]
fn edited_task_revision_cannot_reuse_prior_passing_proof() {
    let mut task = task("done", "agent-a");
    task.revision = 7;
    let prior_verification = bound_verification(&task);
    let prior_claim = from_todo(&task, &prior_verification, None);
    assert_eq!(prior_claim.status, WorkClaimStatus::Complete);

    task.description = "Changed completion requirements".to_string();
    task.revision += 1;
    let stale_claim = from_todo(&task, &prior_verification, None);
    assert_eq!(stale_claim.status, WorkClaimStatus::Blocked);
    assert_eq!(
        stale_claim.validation_status,
        WorkClaimValidationStatus::Stale
    );
    assert_eq!(stale_claim.proof_refs, prior_claim.proof_refs);

    let current_claim = from_todo(&task, &bound_verification(&task), None);
    assert_eq!(current_claim.status, WorkClaimStatus::Complete);
    assert_eq!(
        current_claim.validation_status,
        WorkClaimValidationStatus::Passed
    );
}

#[test]
fn work_claim_projects_lease_fields() {
    let claim = from_todo_with_lease(
        &task("open", "agent-a"),
        &WorkClaimVerification::default(),
        None,
        Some("250Z".to_string()),
        Some("200Z"),
    );
    assert_eq!(claim.lease_expires_at.as_deref(), Some("250Z"));
    assert_eq!(claim.lease_state.as_deref(), Some("active"));

    let expired = from_todo_with_lease(
        &task("open", "agent-a"),
        &WorkClaimVerification::default(),
        None,
        Some("150Z".to_string()),
        Some("200Z"),
    );
    assert_eq!(expired.lease_state.as_deref(), Some("expired"));
}

#[test]
fn dependency_readiness_blocks_active_work_claims() {
    let mut claim = from_todo(
        &task("open", "agent-a"),
        &WorkClaimVerification::default(),
        Some("run-1".to_string()),
    );
    apply_dependency_readiness(
        &mut claim,
        crate::core::fleet_coord::DependencyReadiness {
            state: crate::core::fleet_coord::DependencyReadinessState::ProofBlocked,
            blockers: vec![crate::core::fleet_coord::DependencyBlocker {
                task_id: "feat_01dependency01".to_string(),
                reason: "dependency_proof_not_passed".to_string(),
                status: "done".to_string(),
                validation_status: Some("claimed".to_string()),
            }],
            ..crate::core::fleet_coord::DependencyReadiness::ready()
        },
    );

    assert_eq!(claim.status, WorkClaimStatus::Blocked);
    assert_eq!(
        claim.dependency_readiness.state,
        crate::core::fleet_coord::DependencyReadinessState::ProofBlocked
    );
}
