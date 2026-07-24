// Moved from src/decapod/core/coplayer.rs
use super::*;
use crate::core::trace::{self, TraceEvent};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_resolve_snapshot() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let agent_id = "agent-1";

    // Create some trace events
    let events = vec![
        TraceEvent {
            trace_id: "t1".to_string(),
            ts: "2026-02-19T10:00:00Z".to_string(),
            actor: agent_id.to_string(),
            op: "todo.add".to_string(),
            request: json!({}),
            response: json!({"success": true}),
        },
        TraceEvent {
            trace_id: "t2".to_string(),
            ts: "2026-02-19T10:05:00Z".to_string(),
            actor: agent_id.to_string(),
            op: "todo.claim".to_string(),
            request: json!({}),
            response: json!({"success": true}),
        },
        TraceEvent {
            trace_id: "t3".to_string(),
            ts: "2026-02-19T10:10:00Z".to_string(),
            actor: agent_id.to_string(),
            op: "todo.done".to_string(),
            request: json!({}),
            response: json!({"success": false}),
        },
        TraceEvent {
            trace_id: "t4".to_string(),
            ts: "2026-02-19T10:15:00Z".to_string(),
            actor: "other-agent".to_string(),
            op: "todo.add".to_string(),
            request: json!({}),
            response: json!({"success": true}),
        },
        TraceEvent {
            trace_id: "t5".to_string(),
            ts: "2026-02-19T10:20:00Z".to_string(),
            actor: agent_id.to_string(),
            op: "todo.add".to_string(),
            request: json!({}),
            response: json!({"success": true}),
        },
        TraceEvent {
            trace_id: "t6".to_string(),
            ts: "2026-02-19T10:25:00Z".to_string(),
            actor: agent_id.to_string(),
            op: "todo.add".to_string(),
            request: json!({}),
            response: json!({"success": true}),
        },
    ];

    for ev in events {
        trace::append_trace(root, ev).unwrap();
    }

    let snapshot = resolve_snapshot(root, agent_id).unwrap();

    assert_eq!(snapshot.agent_id, agent_id);
    assert_eq!(snapshot.total_ops, 5);
    assert_eq!(snapshot.successful_ops, 4);
    assert_eq!(snapshot.failed_ops, 1);
    assert_eq!(snapshot.reliability_score, 0.8);
    assert_eq!(snapshot.risk_profile, "medium");
    assert!(snapshot.common_ops.contains(&"todo.add".to_string()));
    assert_eq!(snapshot.last_active, "2026-02-19T10:25:00Z");
}

#[test]
fn test_resolve_snapshot_no_traces() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let snapshot = resolve_snapshot(root, "unknown").unwrap();
    assert_eq!(snapshot.total_ops, 0);
    assert_eq!(snapshot.risk_profile, "unknown");
}

#[test]
fn test_policy_only_tightens() {
    // Policy from a high-reliability agent should still require proofs
    let high_snap = CoPlayerSnapshot {
        agent_id: "trusted-agent".to_string(),
        reliability_score: 0.95,
        total_ops: 100,
        successful_ops: 95,
        failed_ops: 5,
        last_active: "2026-02-19T10:25:00Z".to_string(),
        common_ops: vec!["todo.add".to_string()],
        risk_profile: "low".to_string(),
    };
    let policy = derive_policy(&high_snap);
    // Even high-reliability agents must validate
    assert!(policy.require_validation);
    // High-reliability allows larger diffs but never skips gates
    assert!(policy.max_diff_lines > 0);

    // Unknown agent gets strictly tighter constraints
    let unknown_snap = CoPlayerSnapshot {
        agent_id: "new-agent".to_string(),
        reliability_score: 0.0,
        total_ops: 0,
        successful_ops: 0,
        failed_ops: 0,
        last_active: "never".to_string(),
        common_ops: vec![],
        risk_profile: "unknown".to_string(),
    };
    let unknown_policy = derive_policy(&unknown_snap);
    assert!(unknown_policy.require_validation);
    assert!(unknown_policy.require_handshake);
    // Unknown must have stricter diff limits
    assert!(unknown_policy.max_diff_lines <= policy.max_diff_lines);

    // Low-reliability agent gets even tighter
    let low_snap = CoPlayerSnapshot {
        agent_id: "risky-agent".to_string(),
        reliability_score: 0.5,
        total_ops: 20,
        successful_ops: 10,
        failed_ops: 10,
        last_active: "2026-02-19T10:00:00Z".to_string(),
        common_ops: vec![],
        risk_profile: "high".to_string(),
    };
    let low_policy = derive_policy(&low_snap);
    assert!(low_policy.require_validation);
    assert!(low_policy.require_extra_proofs);
    assert!(low_policy.forbid_broad_refactors);
    // Low-reliability must be strictly tighter than high-reliability
    assert!(low_policy.max_diff_lines <= policy.max_diff_lines);
}
