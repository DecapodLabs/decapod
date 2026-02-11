use crate::core::error;
use crate::core::store::Store;
use crate::health;
use crate::policy;
use crate::watcher;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HeartbeatStatus {
    pub ts: String,
    pub health_summary: std::collections::HashMap<String, usize>, // state -> count
    pub pending_approvals: usize,
    pub watcher_last_run: Option<String>,
}

pub fn get_status(store: &Store) -> Result<HeartbeatStatus, error::DecapodError> {
    health::initialize_health_db(&store.root)?;
    policy::initialize_policy_db(&store.root)?;
    let mut health_summary = std::collections::HashMap::new();
    let all_health = health::get_all_health(store)?;
    for (_, state, _) in all_health {
        let count = health_summary.entry(format!("{:?}", state)).or_insert(0);
        *count += 1;
    }

    let approvals = policy::list_approvals(store).unwrap_or_default();
    // In Epoch 4, "pending" isn't explicitly tracked in policy.db yet,
    // but we can count total approvals as a proxy or just stub.
    let pending_approvals = approvals.len();

    let watcher_events = watcher::watcher_events_path(&store.root);
    let last_run = if watcher_events.exists() {
        let content = fs::read_to_string(watcher_events).unwrap_or_default();
        content.lines().last().and_then(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            v.get("ts").and_then(|t| t.as_str()).map(|s| s.to_string())
        })
    } else {
        None
    };

    Ok(HeartbeatStatus {
        ts: now_iso(),
        health_summary,
        pending_approvals,
        watcher_last_run: last_run,
    })
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}Z", secs)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "heartbeat",
        "version": "0.1.0",
        "description": "Computed system health overview",
        "commands": [
            { "name": "status", "description": "Show health summary, approvals, and watcher status" }
        ],
        "storage": []
    })
}
