use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use crate::plugins::{policy, watcher};
use clap::{Parser, Subcommand};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

pub fn health_db_path(root: &Path) -> PathBuf {
    root.join(schemas::HEALTH_DB_NAME)
}

pub fn initialize_health_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = health_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "health.init", |conn| {
        conn.execute(schemas::HEALTH_DB_SCHEMA_CLAIMS, [])?;
        conn.execute(schemas::HEALTH_DB_SCHEMA_PROOF_EVENTS, [])?;
        conn.execute(schemas::HEALTH_DB_SCHEMA_HEALTH_CACHE, [])?;
        Ok(())
    })
}

#[derive(Parser, Debug)]
#[clap(name = "health", about = "Manage the Health Engine")]
pub struct HealthCli {
    #[clap(subcommand)]
    pub command: HealthCommand,
}

#[derive(Subcommand, Debug)]
pub enum HealthCommand {
    /// Add a new claim to the Health Engine.
    Claim {
        #[clap(long)]
        id: String,
        #[clap(long)]
        subject: String,
        #[clap(long)]
        kind: String,
        #[clap(long, default_value = "")]
        provenance: String,
    },
    /// Record a proof event for a claim.
    Proof {
        #[clap(long)]
        claim_id: String,
        #[clap(long)]
        surface: String,
        #[clap(long)]
        result: String,
        #[clap(long, default_value = "3600")]
        sla: i64,
    },
    /// Get computed health status for a claim.
    Get {
        #[clap(long)]
        id: String,
    },
    /// Show system health summary (aggregates health, policy, watcher status).
    Summary,
    /// Show agent autonomy status based on proof history.
    Autonomy {
        #[clap(long, default_value = "decapod")]
        id: String,
    },
}

pub fn run_health_cli(store: &Store, cli: HealthCli) -> Result<(), error::DecapodError> {
    initialize_health_db(&store.root)?;
    match cli.command {
        HealthCommand::Claim {
            id,
            subject,
            kind,
            provenance,
        } => {
            add_claim(store, &id, &subject, &kind, &provenance)?;
            println!("Claim added: {}", id);
        }
        HealthCommand::Proof {
            claim_id,
            surface,
            result,
            sla,
        } => {
            record_proof(store, &claim_id, &surface, &result, sla)?;
            println!("Proof recorded for: {}", claim_id);
        }
        HealthCommand::Get { id } => {
            let (state, reason) = get_health(store, &id)?;
            println!("Claim: {}\nHealth: {:?}\nReason: {}", id, state, reason);
        }
        HealthCommand::Summary => {
            let summary = get_summary(store)?;
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        HealthCommand::Autonomy { id } => {
            let status = get_autonomy(store, &id)?;
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum HealthState {
    ASSERTED,
    STALE,
    CONTRADICTED,
    VERIFIED,
}

// ===== Summary (formerly heartbeat) =====

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SummaryStatus {
    pub ts: String,
    pub health_summary: std::collections::HashMap<String, usize>, // state -> count
    pub pending_approvals: usize,
    pub watcher_last_run: Option<String>,
    pub watcher_stale: bool,
    pub alerts: Vec<String>,
}

// ===== Autonomy (formerly trust) =====

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum AutonomyTier {
    #[default]
    Untrusted, // Human-only, no agent autonomy
    Basic,    // Confirm all operations
    Verified, // Auto-reversible operations
    Core,     // Full autonomy with trusted operations
}

impl std::fmt::Display for AutonomyTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutonomyTier::Untrusted => write!(f, "untrusted"),
            AutonomyTier::Basic => write!(f, "basic"),
            AutonomyTier::Verified => write!(f, "verified"),
            AutonomyTier::Core => write!(f, "core"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AutonomyStatus {
    pub actor_id: String,
    pub tier: AutonomyTier,
    pub success_count: usize,
    pub failure_count: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claim {
    pub id: String,
    pub subject: String,
    pub kind: String, // FACT | DECISION | TODO
    pub provenance: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProofEvent {
    pub event_id: String,
    pub claim_id: String,
    pub ts: String,
    pub surface: String, // e.g. "cargo test"
    pub result: String,  // "pass" | "fail"
    pub sla_seconds: i64,
}

pub fn compute_health(
    _claim: &Claim,
    events: &[ProofEvent],
    now_secs: i64,
) -> (HealthState, String) {
    if events.is_empty() {
        return (
            HealthState::ASSERTED,
            "No proof events recorded".to_string(),
        );
    }

    // Sort by timestamp descending
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| b.ts.cmp(&a.ts));

    let latest = &sorted_events[0];

    if latest.result == "fail" {
        return (
            HealthState::CONTRADICTED,
            format!("Latest proof failed at {}", latest.ts),
        );
    }

    let last_pass = sorted_events.iter().find(|e| e.result == "pass");

    if let Some(pass) = last_pass {
        let pass_ts: i64 = pass.ts.trim_end_matches('Z').parse().unwrap_or(0);
        if now_secs > pass_ts + pass.sla_seconds {
            return (
                HealthState::STALE,
                format!("Last passing proof ({}) expired SLA", pass.ts),
            );
        }
        return (
            HealthState::VERIFIED,
            format!("Valid proof recorded at {}", pass.ts),
        );
    }

    (
        HealthState::ASSERTED,
        "No passing proof events recorded".to_string(),
    )
}

pub fn add_claim(
    store: &Store,
    id: &str,
    subject: &str,
    kind: &str,
    provenance: &str,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = health_db_path(&store.root);
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "health.claim_add", |conn| {
        conn.execute(
            "INSERT INTO claims(id, subject, kind, provenance, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, subject, kind, provenance, now],
        )?;
        Ok(())
    })
}

pub fn record_proof(
    store: &Store,
    claim_id: &str,
    surface: &str,
    result: &str,
    sla: i64,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = health_db_path(&store.root);
    let now = now_iso();

    broker.with_conn(&db_path, "decapod", None, "health.proof_record", |conn| {
        conn.execute(
            "INSERT INTO proof_events(event_id, claim_id, ts, surface, result, sla_seconds) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![Ulid::new().to_string(), claim_id, now, surface, result, sla],
        )?;
        Ok(())
    })
}

pub fn get_health(
    store: &Store,
    claim_id: &str,
) -> Result<(HealthState, String), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = health_db_path(&store.root);

    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    broker.with_conn(&db_path, "decapod", None, "health.get", |conn| {
        let claim: Claim = conn.query_row(
            "SELECT id, subject, kind, provenance, created_at FROM claims WHERE id = ?1 OR subject = ?1",
            params![claim_id],
            |row| Ok(Claim {
                id: row.get(0)?,
                subject: row.get(1)?,
                kind: row.get(2)?,
                provenance: row.get(3)?,
                created_at: row.get(4)?,
            }),
        ).map_err(|_| error::DecapodError::ValidationError(format!("Claim not found: {}", claim_id)))?;

        let mut stmt = conn.prepare("SELECT event_id, claim_id, ts, surface, result, sla_seconds FROM proof_events WHERE claim_id = ?1")?;
        let event_iter = stmt.query_map(params![claim.id], |row| {
            Ok(ProofEvent {
                event_id: row.get(0)?,
                claim_id: row.get(1)?,
                ts: row.get(2)?,
                surface: row.get(3)?,
                result: row.get(4)?,
                sla_seconds: row.get(5)?,
            })
        })?;

        let events: Vec<ProofEvent> = event_iter.collect::<Result<Vec<_>, _>>().map_err(error::DecapodError::RusqliteError)?;
        let (state, reason) = compute_health(&claim, &events, now);

        // Update cache (non-authoritative)
        conn.execute(
            "INSERT OR REPLACE INTO health_cache(claim_id, computed_state, reason, updated_at) VALUES(?1, ?2, ?3, ?4)",
            params![claim.id, format!("{:?}", state), reason, now_iso()],
        )?;

        Ok((state, reason))
    })
}

pub fn get_all_health(
    store: &Store,
) -> Result<Vec<(String, HealthState, String)>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = health_db_path(&store.root);

    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    broker.with_conn(&db_path, "decapod", None, "health.list_all", |conn| {
        let mut stmt = conn.prepare("SELECT id, subject, kind, provenance, created_at FROM claims")?;
        let claim_iter = stmt.query_map([], |row| {
            Ok(Claim {
                id: row.get(0)?,
                subject: row.get(1)?,
                kind: row.get(2)?,
                provenance: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for claim_res in claim_iter {
            let claim = claim_res?;
            let mut ev_stmt = conn.prepare("SELECT event_id, claim_id, ts, surface, result, sla_seconds FROM proof_events WHERE claim_id = ?1")?;
            let event_iter = ev_stmt.query_map(params![claim.id], |row| {
                Ok(ProofEvent {
                    event_id: row.get(0)?,
                    claim_id: row.get(1)?,
                    ts: row.get(2)?,
                    surface: row.get(3)?,
                    result: row.get(4)?,
                    sla_seconds: row.get(5)?,
                })
            })?;
            let events: Vec<ProofEvent> = event_iter.collect::<Result<Vec<_>, _>>().map_err(error::DecapodError::RusqliteError)?;
            let (state, reason) = compute_health(&claim, &events, now);
            results.push((claim.id, state, reason));
        }
        Ok(results)
    })
}

pub fn get_summary(store: &Store) -> Result<SummaryStatus, error::DecapodError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    initialize_health_db(&store.root)?;
    policy::initialize_policy_db(&store.root)?;

    let mut health_summary = std::collections::HashMap::new();
    let all_health = get_all_health(store)?;
    for (_, state, _) in all_health {
        let count = health_summary.entry(format!("{:?}", state)).or_insert(0);
        *count += 1;
    }

    let approvals = policy::list_approvals(store).unwrap_or_default();
    let pending_approvals = approvals.len();

    let watcher_events = watcher::watcher_events_path(&store.root);
    let (last_run, watcher_stale) = if watcher_events.exists() {
        let content = std::fs::read_to_string(watcher_events).unwrap_or_default();
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

    Ok(SummaryStatus {
        ts: now_iso(),
        health_summary,
        pending_approvals,
        watcher_last_run: last_run,
        watcher_stale,
        alerts,
    })
}

pub fn get_autonomy(store: &Store, actor_id: &str) -> Result<AutonomyStatus, error::DecapodError> {
    initialize_health_db(&store.root)?;

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
            "Actor '{}' has no recorded audit history; autonomy cannot be computed.",
            actor_id
        )));
    }

    // Compute autonomy from proof history
    let all_health = get_all_health(store)?;
    let mut success_count = 0;
    let mut failure_count = 0;

    for (_, state, _) in all_health {
        match state {
            HealthState::VERIFIED => success_count += 1,
            HealthState::CONTRADICTED => failure_count += 1,
            _ => {}
        }
    }

    let mut reasons = Vec::new();
    let tier = if failure_count > 0 {
        reasons.push("Contradicted claims detected; restricted to Basic".to_string());
        AutonomyTier::Basic
    } else if success_count >= 5 {
        reasons.push(format!(
            "Verified success count ({}) exceeds threshold",
            success_count
        ));
        AutonomyTier::Verified
    } else {
        reasons.push("Insufficient verified history for Verified tier".to_string());
        AutonomyTier::Basic
    };

    Ok(AutonomyStatus {
        actor_id: actor_id.to_string(),
        tier,
        success_count,
        failure_count,
        reasons,
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

pub fn claim_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "claim",
        "version": "0.1.0",
        "description": "Manage claims in the Health Engine",
        "commands": [
            { "name": "add", "parameters": ["id", "subject", "kind", "provenance"] }
        ],
        "storage": ["health.db"]
    })
}

pub fn proof_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "proof",
        "version": "0.1.0",
        "description": "Record proof events in the Health Engine",
        "commands": [
            { "name": "record", "parameters": ["claim_id", "surface", "result", "sla"] }
        ],
        "storage": ["health.db"]
    })
}

pub fn health_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "health",
        "version": "0.2.0",
        "description": "Health Engine: claims, proofs, system summary, and agent autonomy",
        "commands": [
            { "name": "claim", "parameters": ["id", "subject", "kind", "provenance"] },
            { "name": "proof", "parameters": ["claim_id", "surface", "result", "sla"] },
            { "name": "get", "parameters": ["id"] },
            { "name": "summary", "description": "System health overview (formerly heartbeat)" },
            { "name": "autonomy", "parameters": ["id"], "description": "Agent autonomy tier (formerly trust)" }
        ],
        "storage": ["health.db"],
        "notes": "Summary consolidates heartbeat; Autonomy consolidates trust"
    })
}
