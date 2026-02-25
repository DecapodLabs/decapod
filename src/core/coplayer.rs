use crate::core::error::DecapodError;
use crate::core::trace::TraceEvent;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoPlayerSnapshot {
    pub agent_id: String,
    pub reliability_score: f64,
    pub total_ops: usize,
    pub successful_ops: usize,
    pub failed_ops: usize,
    pub last_active: String,
    pub common_ops: Vec<String>,
    pub risk_profile: String, // "low", "medium", "high"
}

pub fn resolve_snapshot(
    project_root: &Path,
    agent_id: &str,
) -> Result<CoPlayerSnapshot, DecapodError> {
    let trace_path = project_root.join(".decapod/data/traces.jsonl");
    if !trace_path.exists() {
        return Ok(CoPlayerSnapshot {
            agent_id: agent_id.to_string(),
            reliability_score: 0.0,
            total_ops: 0,
            successful_ops: 0,
            failed_ops: 0,
            last_active: "never".to_string(),
            common_ops: vec![],
            risk_profile: "unknown".to_string(),
        });
    }

    let content = std::fs::read_to_string(trace_path).map_err(DecapodError::IoError)?;
    let mut total = 0;
    let mut success = 0;
    let mut fail = 0;
    let mut last_ts = "unknown".to_string();
    let mut ops_count = std::collections::HashMap::new();

    for line in content.lines() {
        if let Ok(event) = serde_json::from_str::<TraceEvent>(line)
            && event.actor == agent_id {
                total += 1;
                last_ts = event.ts.clone();
                *ops_count.entry(event.op.clone()).or_insert(0) += 1;

                // Simple success detection from RpcResponse format
                if let Some(resp_success) = event.response.get("success").and_then(|v| v.as_bool())
                {
                    if resp_success {
                        success += 1;
                    } else {
                        fail += 1;
                    }
                }
            }
    }

    let reliability = if total > 0 {
        success as f64 / total as f64
    } else {
        0.0
    };

    let mut common_ops: Vec<_> = ops_count.into_iter().collect();
    common_ops.sort_by(|a, b| b.1.cmp(&a.1));
    let common_ops = common_ops.into_iter().take(5).map(|(op, _)| op).collect();

    let risk_profile = if total < 5 {
        "unknown"
    } else if reliability > 0.9 {
        "low"
    } else if reliability > 0.7 {
        "medium"
    } else {
        "high"
    };

    Ok(CoPlayerSnapshot {
        agent_id: agent_id.to_string(),
        reliability_score: reliability,
        total_ops: total,
        successful_ops: success,
        failed_ops: fail,
        last_active: last_ts,
        common_ops,
        risk_profile: risk_profile.to_string(),
    })
}

#[cfg(test)]
mod tests {
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
}

/// Deterministic policy constraints derived from a co-player snapshot.
///
/// INVARIANT: Policies can only TIGHTEN constraints, never loosen them.
/// Every agent, regardless of reliability, MUST pass validation gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoPlayerPolicy {
    /// Agent this policy applies to
    pub agent_id: String,
    /// Validation is always required (never false)
    pub require_validation: bool,
    /// Whether agent must complete handshake before operating
    pub require_handshake: bool,
    /// Whether extra proof steps are required
    pub require_extra_proofs: bool,
    /// Whether broad refactors are forbidden
    pub forbid_broad_refactors: bool,
    /// Maximum diff size (lines) allowed per commit
    pub max_diff_lines: usize,
    /// Risk profile that drove this policy
    pub source_risk_profile: String,
}

/// Derive a deterministic policy from a co-player snapshot.
///
/// This function is the sole authority for snapshot → policy conversion.
/// It uses fixed thresholds and deterministic logic (no stochastic scoring).
///
/// CRITICAL INVARIANT: This function MUST only tighten constraints.
/// - `require_validation` is ALWAYS true.
/// - Lower reliability → stricter limits (never the reverse).
/// - Unknown agents get mandatory handshake + smallest diff limits.
pub fn derive_policy(snapshot: &CoPlayerSnapshot) -> CoPlayerPolicy {
    // Base: every agent must validate. This is non-negotiable.
    let require_validation = true;

    // Unknown or new agents: mandatory handshake, tight limits
    let require_handshake = snapshot.risk_profile == "unknown" || snapshot.total_ops < 5;

    // High-risk agents: extra proofs, no broad refactors
    let require_extra_proofs = snapshot.risk_profile == "high"
        || (snapshot.total_ops >= 5 && snapshot.reliability_score < 0.7);

    let forbid_broad_refactors = snapshot.risk_profile == "high"
        || (snapshot.total_ops >= 5 && snapshot.reliability_score < 0.7);

    // Diff limits: deterministic function of reliability
    // unknown → 100, high-risk → 150, medium → 300, low-risk → 500
    let max_diff_lines = match snapshot.risk_profile.as_str() {
        "unknown" => 100,
        "high" => 150,
        "medium" => 300,
        "low" => 500,
        _ => 100, // Default to strictest
    };

    CoPlayerPolicy {
        agent_id: snapshot.agent_id.clone(),
        require_validation,
        require_handshake,
        require_extra_proofs,
        forbid_broad_refactors,
        max_diff_lines,
        source_risk_profile: snapshot.risk_profile.clone(),
    }
}
