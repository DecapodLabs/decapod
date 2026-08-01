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
    /// Exclusive-claim lease expiry (epoch-Z). Absent on legacy assignments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<String>,
    /// Derived lease classification for fleet coordination consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_state: Option<String>,
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
    from_todo_with_lease(task, verification, trajectory_id, None, None)
}

/// Like [`from_todo`], with optional lease fields for fleet coordination.
pub fn from_todo_with_lease(
    task: &Task,
    verification: &WorkClaimVerification,
    trajectory_id: Option<String>,
    lease_expires_at: Option<String>,
    now_ts: Option<&str>,
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

    let lease_state = now_ts.map(|now| {
        match crate::core::fleet_coord::lease_state(lease_expires_at.as_deref(), now) {
            crate::core::fleet_coord::LeaseState::Active => "active".to_string(),
            crate::core::fleet_coord::LeaseState::Expired => "expired".to_string(),
            crate::core::fleet_coord::LeaseState::Unspecified => "unspecified".to_string(),
        }
    });

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
        lease_expires_at,
        lease_state,
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
#[path = "../../../tests/unit/core/work_claim_tests.rs"]
mod tests;
