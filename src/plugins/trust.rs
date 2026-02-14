//! # DEPRECATED MODULE
//!
//! This module has been deprecated and merged into `health.rs`.
//!
//! ## Migration
//!
//! - **Old**: `decapod trust status --id <agent>`
//! - **New**: `decapod govern health autonomy --id <agent>`
//!
//! The `trust` functionality is now available as the `autonomy` subcommand
//! under `decapod govern health`. All functionality has been preserved.
//!
//! This file is kept for reference only and will be removed in a future version.

#![allow(dead_code)]
#![allow(deprecated)]

use crate::core::error;
use crate::core::store::Store;
use crate::health;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum AutonomyTier {
    Tier0, // Human-only
    Tier1, // Confirm all
    Tier2, // Auto-reversible
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustStatus {
    pub actor_id: String,
    pub tier: AutonomyTier,
    pub success_count: usize,
    pub failure_count: usize,
    pub reasons: Vec<String>,
}

pub fn get_trust_status(store: &Store, actor_id: &str) -> Result<TrustStatus, error::DecapodError> {
    health::initialize_health_db(&store.root)?;

    // Validate actor_id exists in audit history to prevent spoofing
    let audit_log = store.root.join("broker.events.jsonl");
    let mut known_actors = std::collections::HashSet::new();
    known_actors.insert("decapod".to_string());
    if audit_log.exists() {
        let content = std::fs::read_to_string(audit_log).unwrap_or_default();
        for line in content.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(a) = v.get("actor").and_then(|x| x.as_str()) {
                    known_actors.insert(a.to_string());
                }
            }
        }
    }

    if !known_actors.contains(actor_id) {
        return Err(error::DecapodError::ValidationError(format!(
            "Actor '{}' has no recorded audit history; trust cannot be computed.",
            actor_id
        )));
    }

    // In Epoch 4, we compute trust on-the-fly from proof history.
    let all_health = health::get_all_health(store)?;
    let mut success_count = 0;
    let mut failure_count = 0;

    // This is a simplification: we'd ideally filter proof_events by actor_id.
    // Since proof_events table doesn't have actor_id yet, we use global health as a proxy for repo trust.
    for (_, state, _) in all_health {
        match state {
            health::HealthState::VERIFIED => success_count += 1,
            health::HealthState::CONTRADICTED => failure_count += 1,
            _ => {}
        }
    }

    let mut reasons = Vec::new();
    let tier = if failure_count > 0 {
        reasons.push("Contradicted claims detected; restricted to Tier 1".to_string());
        AutonomyTier::Tier1
    } else if success_count >= 5 {
        reasons.push(format!(
            "Verified success count ({}) exceeds threshold",
            success_count
        ));
        AutonomyTier::Tier2
    } else {
        reasons.push("Insufficient verified history for Tier 2".to_string());
        AutonomyTier::Tier1
    };

    Ok(TrustStatus {
        actor_id: actor_id.to_string(),
        tier,
        success_count,
        failure_count,
        reasons,
    })
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "trust",
        "version": "0.1.0",
        "description": "Computed agent autonomy tiers",
        "commands": [
            { "name": "status", "description": "Show computed agent autonomy status", "parameters": ["id"] }
        ],
        "storage": []
    })
}
