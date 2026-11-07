//! Structured, non-authoritative governance projections (issue #870/#871).
//!
//! Projections are deterministic, pure transformations of established Decapod
//! outcomes (RPC responses, validation reports, policy decisions, interlock
//! statuses). They never mutate canonical state, never synthesize success
//! from `not_run`/`unknown`/`stale`, and preserve commit-state uncertainty.

use crate::core::rpc::{BlockerKind, RpcResponse};
use serde::{Deserialize, Serialize};

/// Observed operation status. Derived only from actual response fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Succeeded,
    Failed,
    Blocked,
    Refused,
}

/// Commit outcome of a mutation, preserving uncertainty. `Unknown` is never
/// mapped to success or clean failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitOutcome {
    Committed,
    NotCommitted,
    Unknown,
}

/// Map an interlock status string to a commit outcome. Unrecognized values
/// map to `Unknown` (fail-closed preservation of uncertainty).
pub fn project_interlock_status(status: &str) -> CommitOutcome {
    match status.trim().to_ascii_uppercase().as_str() {
        "COMMITTED" => CommitOutcome::Committed,
        "NOT_COMMITTED" => CommitOutcome::NotCommitted,
        _ => CommitOutcome::Unknown,
    }
}

/// Interlock projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterlockProjection {
    pub code: String,
    pub outcome: CommitOutcome,
}

/// Blocker projection (authority-relevant fields preserved exactly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerProjection {
    pub kind: BlockerKind,
    pub message: String,
    pub resolve_hint: String,
}

/// Receipt reference into the authoritative response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptRef {
    pub op: String,
    pub timestamp: String,
    pub inputs_hash: String,
    pub outputs_hash: String,
}

/// Tool-operation projection of an RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOperationProjection {
    pub op: String,
    pub correlation_id: String,
    /// Task reference when the request carried one (e.g. `todo_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<String>,
    pub phase: String,
    pub status: OperationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touched_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<BlockerProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interlock: Option<InterlockProjection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_next_ops: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorProjection>,
    /// Provenance: where this projection came from. Projections are derived.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorProjection {
    pub code: String,
    pub message: String,
}

/// Classify a response into an observed status without inventing results:
/// `success: true` -> succeeded; blockers present -> blocked; refusal-class
/// error codes -> refused; anything else -> failed.
pub fn classify_response(resp: &RpcResponse) -> OperationStatus {
    if resp.success {
        return OperationStatus::Succeeded;
    }
    if !resp.blocked_by.is_empty() {
        return OperationStatus::Blocked;
    }
    let code = resp.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
    match code {
        "unknown_op"
        | "invalid_params"
        | "negotiation_refused"
        | "unauthorized"
        | "session_required"
        | "mandate_violation"
        | "scope_violation" => OperationStatus::Refused,
        _ => OperationStatus::Failed,
    }
}

/// Pure projection of a real RPC response.
pub fn project_rpc_response(resp: &RpcResponse) -> ToolOperationProjection {
    ToolOperationProjection {
        op: resp.receipt.op.clone(),
        correlation_id: resp.id.clone(),
        task_ref: extract_task_ref(resp),
        phase: "completed".to_string(),
        status: classify_response(resp),
        receipt: Some(ReceiptRef {
            op: resp.receipt.op.clone(),
            timestamp: resp.receipt.timestamp.clone(),
            inputs_hash: resp.receipt.inputs_hash.clone(),
            outputs_hash: resp.receipt.outputs_hash.clone(),
        }),
        touched_paths: resp.receipt.touched_paths.clone(),
        blockers: resp
            .blocked_by
            .iter()
            .map(|b| BlockerProjection {
                kind: b.kind.clone(),
                message: b.message.clone(),
                resolve_hint: b.resolve_hint.clone(),
            })
            .collect(),
        interlock: resp.interlock.as_ref().map(|i| InterlockProjection {
            code: i.code.clone(),
            outcome: project_interlock_status(&i.code),
        }),
        allowed_next_ops: resp.allowed_next_ops.iter().map(|a| a.op.clone()).collect(),
        error: resp.error.as_ref().map(|e| ErrorProjection {
            code: e.code.clone(),
            message: e.message.clone(),
        }),
        source: "rpc_response".to_string(),
    }
}

fn extract_task_ref(resp: &RpcResponse) -> Option<String> {
    // The request params are not echoed in the response; the task reference is
    // taken from governing anchors / touched paths when present, else absent
    // (explicitly missing rather than invented).
    resp.receipt
        .governing_anchors
        .iter()
        .find(|a| a.contains("todo"))
        .cloned()
        .or_else(|| {
            resp.receipt
                .touched_paths
                .iter()
                .find(|p| p.contains("todo"))
                .cloned()
        })
}

/// Approval decision projection. Never synthesizes a granted approval from a
/// displayed selection: `approved: None` stays `pending`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalProjection {
    pub action: String,
    pub correlation_id: Option<String>,
    pub risk_level: Option<String>,
    pub scope: String,
    pub authority_required: bool,
    pub status: ApprovalStatus,
    pub decision_reason: Option<String>,
    pub remediation: Option<String>,
    pub source: String,
}

/// Project an approval decision from real policy inputs.
pub fn project_approval(
    action: &str,
    correlation_id: Option<String>,
    risk_level: Option<String>,
    scope: &str,
    approved: Option<bool>,
    reason: Option<String>,
) -> ApprovalProjection {
    let (status, remediation) = match approved {
        Some(true) => (
            ApprovalStatus::Approved,
            Some("Approval recorded through the governed policy path; scope-limited.".to_string()),
        ),
        Some(false) => (
            ApprovalStatus::Denied,
            Some(
                "Denied by policy; run `decapod govern policy eval` for the approval command."
                    .to_string(),
            ),
        ),
        None => (ApprovalStatus::Pending, None),
    };
    ApprovalProjection {
        action: action.to_string(),
        correlation_id,
        risk_level,
        scope: scope.to_string(),
        authority_required: true,
        status,
        decision_reason: reason,
        remediation,
        source: "policy_decision".to_string(),
    }
}

/// Validation/proof projection. `not_run`/`unknown`/`stale` are never
/// represented as success; blockers are preserved verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationProjection {
    pub validation_id: Option<String>,
    pub epoch_id: Option<String>,
    pub candidate: Option<String>,
    pub status: String,
    pub publication_ready: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
    pub stale: bool,
    /// Proven, not_proven, failed, stale, or unknown. A boolean is not this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_judgment: Option<String>,
    pub source: String,
}

/// Project a `decapod validate --format json` report (or any readiness-shaped
/// JSON) into a typed projection. Unknown/missing fields stay explicitly
/// absent; stale evidence keeps `publication_ready: false`.
pub fn project_validation_report(report: &serde_json::Value) -> ValidationProjection {
    let status = report
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let readiness = report.get("readiness").cloned().unwrap_or_default();
    let blockers: Vec<String> = readiness
        .get("blockers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    b.get("code")
                        .and_then(|c| c.as_str())
                        .map(|c| c.to_string())
                        .or_else(|| {
                            b.get("detail")
                                .and_then(|d| d.as_str())
                                .map(|d| d.to_string())
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    let stale = status == "stale"
        || blockers
            .iter()
            .any(|b| b.contains("stale") || b.contains("STALE"));
    let decision = crate::core::governed_transition::project_report_publication(report);
    ValidationProjection {
        validation_id: report
            .get("validation_id")
            .or_else(|| report.get("run_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        epoch_id: readiness
            .get("epoch_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        candidate: report
            .get("head")
            .or_else(|| report.get("candidate"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        status,
        publication_ready: decision.eligible,
        blockers,
        stale: stale || decision.judgment == crate::core::governed_transition::ProofJudgment::Stale,
        proof_judgment: Some(decision.judgment.as_str().to_string()),
        source: "validation_report".to_string(),
    }
}

/// Request shape for the `interop.project` RPC op. Internally tagged; unknown
/// kinds are rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectRequest {
    RpcResponse {
        response: Box<RpcResponse>,
    },
    ValidationReport {
        report: serde_json::Value,
    },
    InterlockStatus {
        status: String,
    },
    Approval {
        action: String,
        #[serde(default)]
        correlation_id: Option<String>,
        #[serde(default)]
        risk_level: Option<String>,
        #[serde(default = "default_scope")]
        scope: String,
        #[serde(default)]
        approved: Option<bool>,
        #[serde(default)]
        reason: Option<String>,
    },
}

fn default_scope() -> String {
    "global".to_string()
}

/// Apply a projection request. Pure: the input is only read, never stored.
pub fn project(request: &ProjectRequest) -> serde_json::Value {
    match request {
        ProjectRequest::RpcResponse { response } => {
            serde_json::to_value(project_rpc_response(response)).expect("projection serializes")
        }
        ProjectRequest::ValidationReport { report } => {
            serde_json::to_value(project_validation_report(report)).expect("projection serializes")
        }
        ProjectRequest::InterlockStatus { status } => serde_json::to_value(InterlockProjection {
            code: status.clone(),
            outcome: project_interlock_status(status),
        })
        .expect("projection serializes"),
        ProjectRequest::Approval {
            action,
            correlation_id,
            risk_level,
            scope,
            approved,
            reason,
        } => serde_json::to_value(project_approval(
            action,
            correlation_id.clone(),
            risk_level.clone(),
            scope,
            *approved,
            reason.clone(),
        ))
        .expect("projection serializes"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/projection_tests.rs"]
mod tests;
