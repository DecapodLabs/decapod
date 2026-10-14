use crate::core::broker::DbBroker;
use crate::core::db::{self, OptionalExtension, params};
use crate::core::schemas;
use crate::core::store::Store;
use crate::core::{error, events};
use crate::plugins::policy;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

pub fn health_db_path(root: &Path) -> PathBuf {
    root.join(schemas::LOCAL_DB_NAME)
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
            println!("Claim added: {id}");
        }
        HealthCommand::Proof {
            claim_id,
            surface,
            result,
            sla,
        } => {
            record_proof(store, &claim_id, &surface, &result, sla)?;
            println!("Proof recorded for: {claim_id}");
        }
        HealthCommand::Get { id } => {
            let (state, reason) = get_health(store, &id)?;
            println!("Claim: {id}\nHealth: {state:?}\nReason: {reason}");
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

impl fmt::Display for AutonomyTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AutonomyTier::Untrusted => "untrusted",
            AutonomyTier::Basic => "basic",
            AutonomyTier::Verified => "verified",
            AutonomyTier::Core => "core",
        };
        write!(f, "{s}")
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
    claim: &Claim,
    events: &[ProofEvent],
    now_secs: i64,
) -> (HealthState, String) {
    if events.is_empty() {
        return (
            HealthState::ASSERTED,
            "No proof events recorded".to_string(),
        );
    }

    // An invalid timestamp cannot be placed before or after another event.
    // Fail closed rather than discarding it and reviving an older pass.
    if events.iter().any(|event| event.claim_id != claim.id) {
        return (
            HealthState::ASSERTED,
            "Proof event belongs to a different claim".to_string(),
        );
    }
    let mut timed_events = Vec::with_capacity(events.len());
    for event in events {
        let timestamp = event.ts.strip_suffix('Z').unwrap_or(&event.ts);
        let parsed = if !timestamp.is_empty() && timestamp.bytes().all(|c| c.is_ascii_digit()) {
            timestamp.parse::<i64>().ok()
        } else {
            None
        };
        let Some(timestamp) = parsed else {
            return (HealthState::ASSERTED, "Invalid proof timestamp".to_string());
        };
        timed_events.push((timestamp, event));
    }
    let latest_ts = timed_events.iter().map(|(ts, _)| *ts).max().unwrap();
    if latest_ts > now_secs {
        return (
            HealthState::ASSERTED,
            "Latest proof timestamp is in the future".to_string(),
        );
    }
    let latest: Vec<&ProofEvent> = timed_events
        .iter()
        .filter(|(ts, _)| *ts == latest_ts)
        .map(|(_, event)| *event)
        .collect();

    // Epoch timestamps have second resolution; event IDs do not establish
    // insertion order. Any tied failure dominates, independent of row order.
    if latest.iter().any(|event| event.result == "fail") {
        return (
            HealthState::CONTRADICTED,
            format!("Latest proof failed at {latest_ts}Z"),
        );
    }
    if latest.iter().any(|event| event.result != "pass") {
        return (
            HealthState::ASSERTED,
            "Latest proof has an unknown result".to_string(),
        );
    }

    let mut earliest_expiry = i64::MAX;
    for pass in latest {
        let Some(expiry) = latest_ts
            .checked_add(pass.sla_seconds)
            .filter(|_| pass.sla_seconds >= 0)
        else {
            return (
                HealthState::ASSERTED,
                "Latest proof has an invalid SLA".to_string(),
            );
        };
        earliest_expiry = earliest_expiry.min(expiry);
    }
    // All equally recent passes must still be valid. Preserve the inclusive
    // SLA boundary: a proof expires only after its expiry second.
    if now_secs > earliest_expiry {
        return (
            HealthState::STALE,
            format!("Last passing proof ({latest_ts}Z) expired SLA"),
        );
    }
    (
        HealthState::VERIFIED,
        format!("Valid proof recorded at {latest_ts}Z"),
    )
}

fn claim_from_row(row: &db::Row<'_>) -> db::Result<Claim> {
    Ok(Claim {
        id: row.get(0)?,
        subject: row.get(1)?,
        kind: row.get(2)?,
        provenance: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        created_at: row.get(4)?,
    })
}

fn derive_claim_health_from_connection(
    conn: &db::Connection,
    claim: &Claim,
    now_secs: i64,
) -> Result<(HealthState, String), error::DecapodError> {
    if !conn.has_table("proof_events")? {
        return Ok((
            HealthState::ASSERTED,
            "Proof events table is missing".to_string(),
        ));
    }
    let mut stmt = conn.prepare(
        "SELECT event_id, claim_id, ts, surface, result, sla_seconds FROM proof_events WHERE claim_id = ?1",
    )?;
    let events = stmt
        .query_map(params![claim.id], |row| {
            Ok(ProofEvent {
                event_id: row.get(0)?,
                claim_id: row.get(1)?,
                ts: row.get(2)?,
                surface: row.get(3)?,
                result: row.get(4)?,
                sla_seconds: row.get(5)?,
            })
        })?
        .collect::<db::Result<Vec<_>>>()?;
    Ok(compute_health(claim, &events, now_secs))
}

/// Derive current proof authority on the caller's connection and clock.
/// Never reads or writes the advisory cache, opens another connection, or
/// initializes missing tables. Storage errors remain errors, not missing proof.
/// Exact IDs take precedence over subjects; aliases must identify one claim.
pub(crate) fn derive_health_from_connection(
    conn: &db::Connection,
    claim_ref: &str,
    now_secs: i64,
) -> Result<Option<(String, HealthState, String)>, error::DecapodError> {
    if !conn.has_table("claims")? {
        return Ok(None);
    }
    let exact = conn
        .query_row(
            "SELECT id, subject, kind, provenance, created_at FROM claims WHERE id = ?1",
            params![claim_ref],
            claim_from_row,
        )
        .optional()?;
    let claim = if let Some(claim) = exact {
        claim
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, subject, kind, provenance, created_at FROM claims WHERE subject = ?1 ORDER BY id LIMIT 2",
        )?;
        let aliases = stmt
            .query_map(params![claim_ref], claim_from_row)?
            .collect::<db::Result<Vec<_>>>()?;
        if aliases.len() > 1 {
            return Err(error::DecapodError::ValidationError(format!(
                "HEALTH_CLAIM_AMBIGUOUS: subject '{claim_ref}' identifies multiple claims; use an exact claim ID"
            )));
        }
        let Some(claim) = aliases.into_iter().next() else {
            return Ok(None);
        };
        claim
    };
    let (state, reason) = derive_claim_health_from_connection(conn, &claim, now_secs)?;
    Ok(Some((claim.id, state, reason)))
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
            params![crate::core::ulid::new_ulid(), claim_id, now, surface, result, sla],
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

    // This read also refreshes the non-authoritative health cache, so it must
    // use a write connection. Do not classify it as a pure `.get` operation.
    broker.with_conn(&db_path, "decapod", None, "health.inspect", |conn| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let (resolved_id, state, reason) =
            derive_health_from_connection(conn, claim_id, now)?.ok_or_else(|| {
                error::DecapodError::ValidationError(format!(
                    "AUTOREMEDIABLE_VALIDATION_ERROR code=HEALTH_CLAIM_NOT_FOUND severity=transient auto_remediable=true audience=agent agent_action=\"verify the claim ID exists, or create a new claim\" user_note=\"Health claim not found; the agent should locate or create the claim.\"\nClaim not found: {claim_id}"
                ))
            })?;

        // Update cache (non-authoritative)
        conn.execute(
            "INSERT OR REPLACE INTO health_cache(claim_id, computed_state, reason, updated_at) VALUES(?1, ?2, ?3, ?4)",
            params![resolved_id, format!("{:?}", state), reason, format!("{now}Z")],
        )?;

        Ok((state, reason))
    })
}

pub fn get_all_health(
    store: &Store,
) -> Result<Vec<(String, HealthState, String)>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = health_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "health.list_all", |conn| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        if !conn.has_table("claims")? {
            return Ok(Vec::new());
        }
        let mut stmt = conn
            .prepare("SELECT id, subject, kind, provenance, created_at FROM claims ORDER BY id")?;
        let claims = stmt
            .query_map([], claim_from_row)?
            .collect::<db::Result<Vec<_>>>()?;
        let mut results = Vec::with_capacity(claims.len());
        for claim in claims {
            let (state, reason) = derive_claim_health_from_connection(conn, &claim, now)?;
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
        let count = health_summary.entry(format!("{state:?}")).or_insert(0);
        *count += 1;
    }

    let approvals = policy::list_approvals(store).unwrap_or_default();
    let pending_approvals = approvals.len();

    let last_run = events::latest(&store.root, events::WATCHER)?.map(|event| event.ts);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let watcher_stale = last_run
        .as_deref()
        .and_then(|ts| ts.trim_end_matches('Z').parse::<u64>().ok())
        .map(|last_run_secs| now.saturating_sub(last_run_secs) > 600)
        .unwrap_or(true);

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
            "{pending_approvals} pending approvals require review"
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
    let mut known_actors = std::collections::HashSet::new();
    known_actors.insert("decapod".to_string());
    for actor in events::actors(&store.root, events::BROKER)? {
        known_actors.insert(actor);
    }

    if !known_actors.contains(actor_id) {
        return Err(error::DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=HEALTH_ACTOR_NO_AUDIT severity=transient auto_remediable=true audience=agent agent_action=\"ensure the actor has recorded audit history or initialize it\" user_note=\"Actor audit history missing; the agent should verify the actor's presence or create audit entries.\"\nActor '{actor_id}' has no recorded audit history; autonomy cannot be computed."
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
            "Verified success count ({success_count}) exceeds threshold"
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
    crate::core::time::now_epoch_z()
}

pub fn claim_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "claim",
        "version": "0.1.0",
        "description": "Manage claims in the Health Engine",
        "commands": [
            { "name": "add", "parameters": ["id", "subject", "kind", "provenance"] }
        ],
        "storage": ["decapod.db"]
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
        "storage": ["decapod.db"]
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
        "storage": ["decapod.db"],
        "notes": "Summary consolidates heartbeat; Autonomy consolidates trust"
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/plugins/health_tests.rs"]
mod tests;
