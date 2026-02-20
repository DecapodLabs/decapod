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
        if let Ok(event) = serde_json::from_str::<TraceEvent>(line) {
            if event.actor == agent_id {
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
}
