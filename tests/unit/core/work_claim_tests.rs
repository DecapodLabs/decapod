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
    let verification = WorkClaimVerification {
        last_verified_status: Some("pass".to_string()),
        verification_artifacts: None,
    };
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
