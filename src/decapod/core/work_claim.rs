//! Governed work-claim projection for the legacy TODO store.
//!
//! A work claim is the bounded-custody view of a TODO.  The TODO database and
//! event log remain the source of truth for lifecycle and ownership; this
//! projection gives trajectory-aware consumers a stable shape without adding
//! a second store.  Trajectory and workspace ownership stay optional until
//! the corresponding run/branch binding is available.

use crate::core::todo::{Task, WorkClaimVerification};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkClaimStatus {
    Claimed,
    Active,
    Blocked,
    Complete,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkClaimValidationStatus {
    Passed,
    Failed,
    Pending,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkClaim {
    /// Stable projection identity. It is derived from the source TODO ID.
    pub claim_id: String,
    /// The TODO that is being projected; no TODO data is copied into another store.
    pub source_todo_id: String,
    /// Stable local intent anchor for the legacy TODO.
    pub intent_id: String,
    /// Set only when a trajectory explicitly binds this TODO.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trajectory_id: Option<String>,
    /// Reserved for the trajectory loop binding; retries must create a new attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_id: Option<String>,
    pub attempt: u32,
    pub scope: String,
    pub paths: Vec<String>,
    /// Workspace owns these fields; TODO does not infer them from task text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    pub agent: Option<String>,
    pub status: WorkClaimStatus,
    /// References are proof outputs, not proof-plan names.
    pub proof_refs: Vec<String>,
    pub validation_status: WorkClaimValidationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Convert the existing TODO lifecycle and verification record into a claim.
///
/// This mapping is intentionally pure and deterministic. It is the migration
/// contract for existing TODOs and can later be replaced by a persisted claim
/// record without changing the consumer-facing shape.
pub fn from_todo(
    task: &Task,
    verification: &WorkClaimVerification,
    trajectory_id: Option<String>,
) -> WorkClaim {
    let validation_status = validation_status(verification.last_verified_status.as_deref());
    let status = match task.status.as_str() {
        "archived" => WorkClaimStatus::Abandoned,
        "done" if validation_status == WorkClaimValidationStatus::Passed => {
            WorkClaimStatus::Complete
        }
        "done" => WorkClaimStatus::Blocked,
        "blocked" => WorkClaimStatus::Blocked,
        _ if validation_status == WorkClaimValidationStatus::Failed => WorkClaimStatus::Blocked,
        _ if task.assigned_to.is_empty() => WorkClaimStatus::Claimed,
        _ => WorkClaimStatus::Active,
    };

    let mut paths = Vec::new();
    if !task.dir_path.trim().is_empty() {
        paths.push(task.dir_path.clone());
    }

    WorkClaim {
        claim_id: format!("todo:{}", task.id),
        source_todo_id: task.id.clone(),
        intent_id: format!("intent:todo:{}", task.id),
        trajectory_id,
        loop_id: None,
        attempt: 1,
        scope: if task.scope.trim().is_empty() {
            "repo".to_string()
        } else {
            task.scope.clone()
        },
        paths,
        branch: None,
        worktree: None,
        agent: (!task.assigned_to.is_empty()).then(|| task.assigned_to.clone()),
        status,
        proof_refs: proof_refs(&task.id, verification.verification_artifacts.as_ref()),
        validation_status,
        external_ref: (!task.r#ref.is_empty()).then(|| task.r#ref.clone()),
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
    }
}

fn validation_status(status: Option<&str>) -> WorkClaimValidationStatus {
    match status.map(|value| value.to_ascii_lowercase()).as_deref() {
        Some("pass" | "passed" | "verified") => WorkClaimValidationStatus::Passed,
        Some("fail" | "failed") => WorkClaimValidationStatus::Failed,
        Some("claimed" | "pending") => WorkClaimValidationStatus::Pending,
        Some(_) => WorkClaimValidationStatus::Unknown,
        None => WorkClaimValidationStatus::Missing,
    }
}

fn proof_refs(todo_id: &str, artifacts: Option<&Value>) -> Vec<String> {
    let Some(object) = artifacts.and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut refs = Vec::new();
    if let Some(results) = object.get("proof_plan_results").and_then(Value::as_array) {
        for result in results {
            let Some(result) = result.as_object() else {
                continue;
            };
            let Some(gate) = result.get("proof_gate").and_then(Value::as_str) else {
                continue;
            };
            let Some(hash) = result.get("output_hash").and_then(Value::as_str) else {
                continue;
            };
            refs.push(format!("proof:{todo_id}:{gate}:{hash}"));
        }
    }
    if let Some(artifacts) = object.get("file_artifacts").and_then(Value::as_array) {
        for artifact in artifacts {
            let Some(artifact) = artifact.as_object() else {
                continue;
            };
            let Some(path) = artifact.get("path").and_then(Value::as_str) else {
                continue;
            };
            let Some(hash) = artifact.get("hash").and_then(Value::as_str) else {
                continue;
            };
            refs.push(format!("artifact:{path}:{hash}"));
        }
    }
    refs.sort();
    refs.dedup();
    refs
}

#[cfg(test)]
mod tests {
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
}
