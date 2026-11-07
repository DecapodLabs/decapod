use super::*;
use crate::core::rpc::{Blocker, BlockerKind, Receipt, RpcError, RpcResponse};

fn response(success: bool, error_code: Option<&str>, blockers: Vec<Blocker>) -> RpcResponse {
    RpcResponse {
        id: "req-1".to_string(),
        success,
        receipt: Receipt {
            op: "store.upsert".to_string(),
            timestamp: "2026-11-06T00:00:00Z".to_string(),
            inputs_hash: "sha256:aa".to_string(),
            outputs_hash: "sha256:bb".to_string(),
            touched_paths: vec!["knowledge/x".to_string()],
            governing_anchors: vec![],
        },
        mandates: vec![],
        context_capsule: None,
        result: None,
        allowed_next_ops: vec![],
        blocked_by: blockers,
        interlock: None,
        advisory: None,
        attestation: None,
        error: error_code.map(|code| RpcError {
            code: code.to_string(),
            message: "msg".to_string(),
            details: None,
        }),
    }
}

#[test]
fn interlock_unknown_stays_unknown() {
    assert_eq!(project_interlock_status("UNKNOWN"), CommitOutcome::Unknown);
    assert_eq!(project_interlock_status("whatever"), CommitOutcome::Unknown);
    assert_eq!(
        project_interlock_status("COMMITTED"),
        CommitOutcome::Committed
    );
    assert_eq!(
        project_interlock_status("NOT_COMMITTED"),
        CommitOutcome::NotCommitted
    );
    // case-insensitive
    assert_eq!(
        project_interlock_status("committed"),
        CommitOutcome::Committed
    );
}

#[test]
fn projection_of_success_is_succeeded() {
    let resp = response(true, None, vec![]);
    let proj = project_rpc_response(&resp);
    assert_eq!(proj.status, OperationStatus::Succeeded);
    assert_eq!(proj.op, "store.upsert");
    assert_eq!(proj.correlation_id, "req-1");
    let receipt = proj.receipt.expect("receipt preserved");
    assert_eq!(receipt.inputs_hash, "sha256:aa");
}

#[test]
fn projection_of_refusal_stays_refused_not_success() {
    for code in [
        "unknown_op",
        "invalid_params",
        "negotiation_refused",
        "unauthorized",
    ] {
        let resp = response(false, Some(code), vec![]);
        let proj = project_rpc_response(&resp);
        assert_eq!(proj.status, OperationStatus::Refused, "code {code}");
        assert_eq!(proj.error.as_ref().unwrap().code, code);
    }
}

#[test]
fn projection_of_blocked_stays_blocked_with_blockers_preserved() {
    let resp = response(
        false,
        Some("mandate_violation"),
        vec![Blocker {
            kind: BlockerKind::MissingProof,
            message: "proof missing".to_string(),
            resolve_hint: "run validate".to_string(),
        }],
    );
    let proj = project_rpc_response(&resp);
    // blockers take precedence over error-class mapping
    assert_eq!(proj.status, OperationStatus::Blocked);
    assert_eq!(proj.blockers.len(), 1);
    assert!(matches!(proj.blockers[0].kind, BlockerKind::MissingProof));
    assert_eq!(proj.blockers[0].resolve_hint, "run validate");
}

#[test]
fn projection_of_failure_is_failed() {
    let resp = response(false, Some("internal_error"), vec![]);
    let proj = project_rpc_response(&resp);
    assert_eq!(proj.status, OperationStatus::Failed);
}

#[test]
fn approval_projection_never_synthesizes_grant() {
    let pending = project_approval(
        "external:verification_exec:x",
        None,
        Some("high".to_string()),
        "global",
        None,
        None,
    );
    assert_eq!(pending.status, ApprovalStatus::Pending);
    assert!(pending.authority_required);
    let denied = project_approval(
        "a",
        Some("c".into()),
        None,
        "global",
        Some(false),
        Some("risk".into()),
    );
    assert_eq!(denied.status, ApprovalStatus::Denied);
    assert_eq!(denied.decision_reason.as_deref(), Some("risk"));
    let approved = project_approval("a", None, None, "global", Some(true), None);
    assert_eq!(approved.status, ApprovalStatus::Approved);
}

#[test]
fn validation_projection_never_turns_stale_or_unknown_into_ready() {
    let stale_report = serde_json::json!({
        "status": "fail",
        "publication_ready": true,
        "readiness": { "blockers": [ { "code": "STALE_EPOCH", "detail": "stale fingerprint abc" } ] }
    });
    let proj = project_validation_report(&stale_report);
    assert!(proj.stale);
    assert!(
        !proj.publication_ready,
        "stale evidence cannot be publication-ready"
    );
    assert_eq!(proj.blockers, vec!["STALE_EPOCH"]);

    let unknown_report = serde_json::json!({});
    let proj = project_validation_report(&unknown_report);
    assert_eq!(proj.status, "unknown");
    assert!(!proj.publication_ready);
    assert!(
        proj.validation_id.is_none(),
        "missing fields stay explicitly absent"
    );

    let pass_report = serde_json::json!({
        "status": "pass",
        "publication_ready": true,
        "head": "abc123abc123",
        "receipt_hash": "sha256:0123456789abcdef",
        "readiness": { "epoch_id": "epoch-1", "blockers": [] }
    });
    let proj = project_validation_report(&pass_report);
    assert_eq!(proj.status, "pass");
    assert!(proj.publication_ready);
    assert!(!proj.stale);
    assert_eq!(proj.proof_judgment.as_deref(), Some("proven"));
    assert_eq!(proj.candidate.as_deref(), Some("abc123abc123"));
    assert_eq!(proj.epoch_id.as_deref(), Some("epoch-1"));

    let declared = serde_json::json!({
        "status": "done",
        "passes": true,
        "publication_ready": true
    });
    let declared_proj = project_validation_report(&declared);
    assert!(!declared_proj.publication_ready);
    assert_ne!(declared_proj.proof_judgment.as_deref(), Some("proven"));
}

#[test]
fn project_request_is_pure_and_serializable() {
    let resp = response(true, None, vec![]);
    let req = ProjectRequest::RpcResponse {
        response: Box::new(resp),
    };
    let v1 = project(&req);
    let v2 = project(&req);
    assert_eq!(v1, v2, "deterministic");
    assert_eq!(v1["status"], "succeeded");
    assert_eq!(v1["source"], "rpc_response");

    let req = ProjectRequest::InterlockStatus {
        status: "UNKNOWN".to_string(),
    };
    let v = project(&req);
    assert_eq!(v["outcome"], "unknown");

    // Malformed kind is rejected by serde, not silently coerced.
    let bad = serde_json::json!({ "kind": "not_a_kind" });
    assert!(serde_json::from_value::<ProjectRequest>(bad).is_err());
}
