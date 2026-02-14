//! # DEPRECATED MODULE
//!
//! This module has been deprecated and merged into `health.rs`.
//!
//! ## Migration
//!
//! - **Old**: `decapod heartbeat`
//! - **New**: `decapod govern health summary`
//!
//! The `heartbeat` functionality is now available as the `summary` subcommand
//! under `decapod govern health`. All functionality has been preserved.
//!
//! This file is kept for reference only and will be removed in a future version.

#![allow(dead_code)]
#![allow(deprecated)]

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
    pub watcher_stale: bool,
    pub alerts: Vec<String>,
}

pub fn get_status(store: &Store) -> Result<HeartbeatStatus, error::DecapodError> {
    use std::time::{SystemTime, UNIX_EPOCH};

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
    let (last_run, watcher_stale) = if watcher_events.exists() {
        let content = fs::read_to_string(watcher_events).unwrap_or_default();
        let last_line = content.lines().last();
        let last_ts = last_line.and_then(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            v.get("ts").and_then(|t| t.as_str()).map(|s| s.to_string())
        });

        // Check if watcher is stale (> 10 minutes since last run)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let is_stale = match &last_ts {
            None => true,
            Some(ts) => ts
                .trim_end_matches('Z')
                .parse::<u64>()
                .map(|last_run_secs| now.saturating_sub(last_run_secs) > 600)
                .unwrap_or(true),
        };

        (last_ts, is_stale)
    } else {
        (None, true)
    };

    // Build alerts
    let mut alerts = Vec::new();
    if watcher_stale {
        alerts.push(
            "Watcher has not run recently (> 10 minutes). Run: decapod govern watcher run"
                .to_string(),
        );
    }
    if health_summary.get("CONTRADICTED").unwrap_or(&0) > &0 {
        alerts.push(
            "Some health claims are contradicted. Check: decapod govern health get".to_string(),
        );
    }
    if health_summary.get("STALE").unwrap_or(&0) > &0 {
        alerts.push("Some health claims are stale. Run: decapod govern proof run".to_string());
    }
    if pending_approvals > 0 {
        alerts.push(format!(
            "{} pending approvals require review",
            pending_approvals
        ));
    }

    Ok(HeartbeatStatus {
        ts: now_iso(),
        health_summary,
        pending_approvals,
        watcher_last_run: last_run,
        watcher_stale,
        alerts,
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
